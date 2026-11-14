import { useId, useState, type ReactNode } from "react";
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis, YAxis, type TooltipContentProps } from "recharts";

import type { DataBucket } from "../api/client";
import { PHONE, useMedia } from "./media";

/** One stack of a chart: what it counts, its colour (a design token), and its value in a bucket. */
export interface DataSeries {
  key: string;
  label: string;
  color: string;
  value: (bucket: DataBucket) => number;
}

type Row = Record<string, number | string | boolean>;

/**
 * One chart of the Data page (ux.md, Data): a bar per bucket, stacked when there are several series. Hovering or
 * focusing a bar shows its values; on a phone a tap shows them as a line under the title instead. Every number is also
 * in a table for screen readers.
 */
export function DataChart({
  title,
  buckets,
  series,
  format = String,
  empty,
}: {
  title: string;
  buckets: DataBucket[];
  series: DataSeries[];
  format?: (value: number) => string;
  /** What to say instead of bars when nothing happened in the whole period. */
  empty: string | null;
}) {
  const titleId = useId();
  const phone = useMedia(PHONE);
  const [picked, setPicked] = useState<number | undefined>();
  const rows: Row[] = buckets.map((bucket) => ({
    label: bucket.label,
    partial: bucket.partial,
    ...Object.fromEntries(series.map((s) => [s.key, s.value(bucket)])),
  }));
  const pickedRow = picked === undefined ? undefined : rows[picked];

  return (
    <section aria-labelledby={titleId} className="rounded-[14px] border border-line bg-surface p-3">
      <h2 id={titleId} className="text-sm font-semibold">
        {title}
      </h2>
      {phone && (
        <p className="min-h-5 text-[13px] text-ink-soft">
          {pickedRow ? describe(pickedRow, series, format) : "Tap a bar for its numbers."}
        </p>
      )}
      {empty !== null ? (
        <p className="flex h-[180px] items-center justify-center text-sm text-muted">{empty}</p>
      ) : (
        <>
          <div className="mt-2 h-[180px]">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart
                data={rows}
                title={title}
                margin={{ top: 4, right: 4, bottom: 0, left: 0 }}
                onClick={(state) => {
                  const index = Number(state.activeTooltipIndex);
                  setPicked(Number.isInteger(index) ? index : undefined);
                }}
              >
                <XAxis
                  dataKey="label"
                  tick={{ fontSize: 11, fill: "var(--muted)" }}
                  tickLine={false}
                  interval="preserveStartEnd"
                />
                <YAxis
                  width={40}
                  tickCount={4}
                  allowDecimals={false}
                  tick={{ fontSize: 11, fill: "var(--muted)" }}
                  tickLine={false}
                  axisLine={false}
                  tickFormatter={(value: number) => format(value)}
                />
                <Tooltip
                  cursor={{ fill: "var(--sunken)" }}
                  wrapperStyle={phone ? { display: "none" } : undefined}
                  content={(props: TooltipContentProps) => (
                    <TooltipBody {...props} rows={rows} series={series} format={format} />
                  )}
                />
                {series.map((s) => (
                  <Bar key={s.key} dataKey={s.key} stackId="stack" fill={s.color} isAnimationActive={false} />
                ))}
              </BarChart>
            </ResponsiveContainer>
          </div>
          {series.length > 1 && (
            <ul className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-ink-soft">
              {series.map((s) => (
                <li key={s.key} className="flex items-center gap-1.5">
                  <span aria-hidden="true" className="size-2.5 rounded-sm" style={{ background: s.color }} />
                  {s.label}
                </li>
              ))}
            </ul>
          )}
          <table className="sr-only">
            <caption>{title}</caption>
            <thead>
              <tr>
                <th scope="col">Period</th>
                {series.map((s) => (
                  <th key={s.key} scope="col">
                    {s.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={String(row.label)}>
                  <th scope="row">
                    {row.label}
                    {row.partial ? " · so far" : ""}
                  </th>
                  {series.map((s) => (
                    <td key={s.key}>{format(Number(row[s.key]))}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}

/** "14 Nov · so far: 2 done, 1 won't do". */
function describe(row: Row, series: DataSeries[], format: (value: number) => string): string {
  const head = `${row.label}${row.partial ? " · so far" : ""}`;
  return `${head}: ${series.map((s) => `${format(Number(row[s.key]))} ${s.label.toLowerCase()}`).join(", ")}`;
}

function TooltipBody({
  active,
  label,
  rows,
  series,
  format,
}: TooltipContentProps & {
  rows: Row[];
  series: DataSeries[];
  format: (value: number) => string;
}): ReactNode {
  const row = rows.find((r) => r.label === label);
  if (!active || !row) return null;
  return (
    <div className="rounded-lg border border-line bg-surface px-2.5 py-1.5 text-xs text-ink shadow-md">
      <p className="font-semibold">
        {row.label}
        {row.partial ? " · so far" : ""}
      </p>
      {series.map((s) => (
        <p key={s.key} className="flex items-center gap-1.5">
          <span aria-hidden="true" className="size-2.5 rounded-sm" style={{ background: s.color }} />
          {s.label}: <span className="tabular-nums">{format(Number(row[s.key]))}</span>
        </p>
      ))}
    </div>
  );
}
