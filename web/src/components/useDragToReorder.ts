import type { PointerEvent as ReactPointerEvent, RefObject } from "react";
import { useCallback, useEffect, useRef, useState } from "react";

/** The pointer handlers of a row's grip. */
export type GripProps = {
  onPointerDown: (event: ReactPointerEvent<HTMLElement>) => void;
  onPointerMove: (event: ReactPointerEvent<HTMLElement>) => void;
  onPointerUp: (event: ReactPointerEvent<HTMLElement>) => void;
  onPointerCancel: (event: ReactPointerEvent<HTMLElement>) => void;
};

/** How near the edge of the scrolling pane the pointer scrolls it, and by how much a frame. */
const EDGE_ZONE = 48;
const EDGE_SCROLL = 8;
/** How far the pointer moves before a press on the grip is a drag, so a click near the pane's edge scrolls nothing. */
const DRAG_START = 6;
/** How long a finger stays within how far of where it went down before the press lifts a row. */
const PRESS_TIME = 400;
const PRESS_SLOP = 8;

/**
 * Dragging a row by its grip, with pointer events (mouse, touch and pen alike). The dragged row follows the pointer
 * (`dy`, for `lifted`) while the others stay, and `lineY` is where the landing place's line goes, within the list. The
 * landing place is the number of other rows whose midpoint is above the pointer; the rows' rects are read again at
 * every move and frame, since the pane may be scrolling. Escape and a cancelled pointer put the row back.
 */
export function useDragToReorder<Id>({
  listRef,
  rows,
  ids,
  onDrop,
}: {
  listRef: RefObject<HTMLElement | null>;
  rows: RefObject<Map<Id, HTMLElement>>;
  /** The rows' ids in the shown order. */
  ids: readonly Id[];
  /** Called with the row and the index it lands at in the shown order. */
  onDrop: (id: Id, index: number) => void;
}) {
  const [lifted, setLifted] = useState<Id | null>(null);
  const [dy, setDy] = useState(0);
  const [lineY, setLineY] = useState<number | null>(null);
  const session = useRef<{
    id: Id;
    pointerId: number;
    startY: number;
    startListTop: number;
    pointerY: number;
    others: readonly Id[];
    index: number;
    landing: number;
    scroller: Element;
    /** Whether the pointer has gone past `DRAG_START` from where it went down; until then a press is not a drag. */
    moved: boolean;
  } | null>(null);

  const measure = useCallback(() => {
    const current = session.current;
    const list = listRef.current;
    if (!current || !list) return;
    const listTop = list.getBoundingClientRect().top;
    // The list moves when its pane scrolls, and the row has to stay under the pointer.
    setDy(current.pointerY - current.startY + current.startListTop - listTop);
    const rects = current.others.map((id) => rows.current.get(id)?.getBoundingClientRect());
    const landing = rects.filter((rect) => rect && (rect.top + rect.bottom) / 2 < current.pointerY).length;
    current.landing = landing;
    const above = rects[landing - 1];
    const below = rects[landing];
    if (landing === current.index) setLineY(null);
    else if (above && below) setLineY((above.bottom + below.top) / 2 - listTop);
    else if (below) setLineY(below.top - listTop);
    else if (above) setLineY(above.bottom - listTop);
  }, [listRef, rows]);

  // A long press waiting for its time, and whether the lifted row came from one.
  const press = useRef<{ pointerId: number; startX: number; startY: number; y: number; timer: number } | null>(null);
  const [byPress, setByPress] = useState(false);
  // Whether the latest press went on to drag, so a control under the pointer can tell the end of a drag from a click.
  const dragged = useRef(false);
  const clearPress = () => {
    if (press.current) window.clearTimeout(press.current.timer);
    press.current = null;
  };

  const end = useCallback(() => {
    session.current = null;
    setByPress(false);
    setLifted(null);
    setDy(0);
    setLineY(null);
  }, []);

  useEffect(() => {
    if (lifted === null) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // Only the drag takes this Escape: the item stays open.
      event.preventDefault();
      event.stopPropagation();
      end();
    };
    window.addEventListener("keydown", onKeyDown, true);
    let frame = requestAnimationFrame(function scroll() {
      const current = session.current;
      if (current) {
        const edge =
          current.scroller === document.scrollingElement
            ? { top: 0, bottom: window.innerHeight }
            : current.scroller.getBoundingClientRect();
        if (current.pointerY < edge.top + EDGE_ZONE) current.scroller.scrollTop -= EDGE_SCROLL;
        else if (current.pointerY > edge.bottom - EDGE_ZONE) current.scroller.scrollTop += EDGE_SCROLL;
        measure();
      }
      frame = requestAnimationFrame(scroll);
    });
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      cancelAnimationFrame(frame);
    };
  }, [lifted, measure, end]);

  // Starts a session for the row `id` on the pointer `pointerId`, captured by `element`. A session that starts `held` is
  // lifted at once: a long press has gone past `DRAG_START` already.
  const begin = (id: Id, element: Element, pointerId: number, clientY: number, held: boolean) => {
    if (session.current || !listRef.current) return;
    element.setPointerCapture(pointerId);
    let scroller: Element = document.scrollingElement ?? document.documentElement;
    for (let node = listRef.current.parentElement; node; node = node.parentElement) {
      const { overflowY } = getComputedStyle(node);
      if (/auto|scroll|overlay/.test(overflowY) && node.scrollHeight > node.clientHeight) {
        scroller = node;
        break;
      }
    }
    const index = ids.indexOf(id);
    session.current = {
      id,
      pointerId,
      startY: clientY,
      startListTop: listRef.current.getBoundingClientRect().top,
      pointerY: clientY,
      others: ids.filter((other) => other !== id),
      index,
      landing: index,
      scroller,
      moved: held,
    };
    dragged.current = held;
    if (held) setLifted(id);
  };

  const handlers = {
    onPointerMove: (event: ReactPointerEvent<HTMLElement>) => {
      if (session.current?.pointerId !== event.pointerId) return;
      session.current.pointerY = event.clientY;
      if (!session.current.moved && Math.abs(event.clientY - session.current.startY) > DRAG_START) {
        session.current.moved = true;
        dragged.current = true;
        setLifted(session.current.id);
      }
      measure();
    },
    onPointerUp: (event: ReactPointerEvent<HTMLElement>) => {
      const current = session.current;
      if (current?.pointerId !== event.pointerId) return;
      measure();
      end();
      if (current.moved && current.landing !== current.index) onDrop(current.id, current.landing);
    },
    onPointerCancel: (event: ReactPointerEvent<HTMLElement>) => {
      if (session.current?.pointerId === event.pointerId) end();
    },
  };

  // A lifted row is dragged by the finger, not the page: the page doesn't scroll by touch meanwhile.
  useEffect(() => {
    if (!byPress) return;
    const stop = (event: TouchEvent) => event.preventDefault();
    document.addEventListener("touchmove", stop, { passive: false });
    return () => document.removeEventListener("touchmove", stop);
  }, [byPress]);

  useEffect(() => clearPress, []);

  return {
    active: lifted !== null,
    lifted,
    dy,
    lineY,
    /** Whether the latest press went on to drag. */
    dragged: () => dragged.current,
    gripProps: (id: Id): GripProps => ({
      ...handlers,
      onPointerDown: (event) => {
        if (event.button !== 0) return;
        begin(id, event.currentTarget, event.pointerId, event.clientY, false);
      },
    }),
    /**
     * The handlers of a row that lifts after a long press: the pointer stays within `PRESS_SLOP` for `PRESS_TIME`, then
     * the row is lifted and follows the finger as a grip's row does. A move past the slop before then, a lift of the
     * finger or a cancelled pointer ends the press, so a swipe or a scroll goes on as it would.
     */
    pressProps: (id: Id): GripProps => ({
      ...handlers,
      onPointerDown: (event) => {
        if (event.button !== 0 || session.current) return;
        const element = event.currentTarget;
        const { pointerId, clientX, clientY } = event;
        clearPress();
        press.current = {
          pointerId,
          startX: clientX,
          startY: clientY,
          y: clientY,
          timer: window.setTimeout(() => {
            // Where the finger is by now.
            const y = press.current?.y ?? clientY;
            press.current = null;
            begin(id, element, pointerId, y, true);
            setByPress(true);
          }, PRESS_TIME),
        };
      },
      onPointerMove: (event) => {
        const current = press.current;
        if (current?.pointerId === event.pointerId) {
          current.y = event.clientY;
          if (Math.hypot(event.clientX - current.startX, event.clientY - current.startY) > PRESS_SLOP) clearPress();
        }
        handlers.onPointerMove(event);
      },
      onPointerUp: (event) => {
        clearPress();
        handlers.onPointerUp(event);
      },
      onPointerCancel: (event) => {
        clearPress();
        handlers.onPointerCancel(event);
      },
    }),
  };
}
