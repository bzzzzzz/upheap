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

  const end = useCallback(() => {
    session.current = null;
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

  return {
    active: lifted !== null,
    lifted,
    dy,
    lineY,
    gripProps: (id: Id): GripProps => ({
      onPointerDown: (event) => {
        if (event.button !== 0 || session.current || !listRef.current) return;
        event.currentTarget.setPointerCapture(event.pointerId);
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
          pointerId: event.pointerId,
          startY: event.clientY,
          startListTop: listRef.current.getBoundingClientRect().top,
          pointerY: event.clientY,
          others: ids.filter((other) => other !== id),
          index,
          landing: index,
          scroller,
          moved: false,
        };
      },
      onPointerMove: (event) => {
        if (session.current?.pointerId !== event.pointerId) return;
        session.current.pointerY = event.clientY;
        if (!session.current.moved && Math.abs(event.clientY - session.current.startY) > DRAG_START) {
          session.current.moved = true;
          setLifted(session.current.id);
        }
        measure();
      },
      onPointerUp: (event) => {
        const current = session.current;
        if (current?.pointerId !== event.pointerId) return;
        measure();
        end();
        if (current.moved && current.landing !== current.index) onDrop(current.id, current.landing);
      },
      onPointerCancel: (event) => {
        if (session.current?.pointerId === event.pointerId) end();
      },
    }),
  };
}
