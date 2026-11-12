import { memo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

/**
 * Text written by other people is shown here, never run: raw HTML is skipped (`skipHtml`, and no `rehype-raw`),
 * react-markdown's default URL transform stays on so a `javascript:` link is dropped, and an image is never loaded from
 * a remote address, which would tell its host who read the item. It reads as a link to the image instead.
 */
const components: Components = {
  // The item's own headings are h2 and h3, so the text's go below them, and all read small.
  h1: ({ children }) => <h4 className="mt-1 text-[15px] font-semibold first:mt-0">{children}</h4>,
  h2: ({ children }) => <h4 className="mt-1 text-sm font-semibold first:mt-0">{children}</h4>,
  h3: ({ children }) => <h4 className="mt-1 text-sm font-semibold first:mt-0">{children}</h4>,
  h4: ({ children }) => <h5 className="mt-1 text-sm font-semibold first:mt-0">{children}</h5>,
  h5: ({ children }) => <h5 className="mt-1 text-sm font-semibold first:mt-0">{children}</h5>,
  h6: ({ children }) => <h5 className="mt-1 text-sm font-semibold first:mt-0">{children}</h5>,
  a: ({ href, children }) => {
    // A link the URL transform dropped (an unsafe one) reads as the text it was written over.
    if (href === undefined || href === "") return <span>{children}</span>;
    // An in-page anchor, such as a footnote's, moves in place; every other link opens a tab, so the app stays open.
    const inPage = href.startsWith("#");
    return (
      <a
        href={href}
        {...(inPage ? {} : { target: "_blank", rel: "noopener noreferrer" })}
        className="text-accent-ink underline"
      >
        {children}
      </a>
    );
  },
  img: ({ src, alt }) =>
    typeof src === "string" && src !== "" ? (
      <a href={src} target="_blank" rel="noopener noreferrer" className="text-accent-ink underline">
        {alt || src}
      </a>
    ) : (
      <>{alt}</>
    ),
  ul: ({ className, children }) => (
    <ul className={className?.includes("contains-task-list") ? "flex flex-col gap-0.5" : "list-disc pl-5"}>
      {children}
    </ul>
  ),
  ol: ({ children }) => <ol className="list-decimal pl-5">{children}</ol>,
  input: ({ checked }) => <input type="checkbox" checked={checked ?? false} disabled readOnly className="mr-1.5" />,
  blockquote: ({ children }) => (
    <blockquote className="flex flex-col gap-2 border-l-2 border-line-strong pl-3 text-ink-soft">{children}</blockquote>
  ),
  hr: () => <hr className="border-line" />,
  pre: ({ children }) => (
    <pre className="overflow-x-auto rounded-lg bg-sunken p-2 font-mono text-[13px] [&>code]:bg-transparent [&>code]:p-0">
      {children}
    </pre>
  ),
  code: ({ children }) => <code className="rounded bg-sunken px-1 font-mono text-[13px]">{children}</code>,
  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table className="border-collapse text-left">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border border-line px-2 py-1 font-semibold">{children}</th>,
  td: ({ children }) => <td className="border border-line px-2 py-1">{children}</td>,
};

const remarkPlugins = [remarkGfm, remarkBreaks];

/**
 * Markdown text written by people and agents (ux.md, Markdown text), read-only. GitHub-flavoured, with a single newline
 * kept as a line break so text written as plain text reads as it was typed. `className` sets the text colour. Memoised
 * on the text, so a render that leaves the text alone doesn't parse it again.
 */
export const Markdown = memo(function Markdown({ text, className = "text-ink" }: { text: string; className?: string }) {
  return (
    <div className={`flex flex-col gap-2 text-sm leading-normal break-words ${className}`}>
      <ReactMarkdown remarkPlugins={remarkPlugins} skipHtml components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
