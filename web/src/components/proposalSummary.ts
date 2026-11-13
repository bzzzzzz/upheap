import type { Item } from "../api/client";

/** What the owner needs to decide a done proposal: how far the steps are, and how the reviews went. */
export function proposalSummary(item: Item): string | null {
  const parts: string[] = [];
  const progress = item.step_progress;
  if (progress && progress.total > 0) parts.push(`Steps ${progress.ticked}/${progress.total}`);

  // The latest round of each piece decides it.
  const latest = new Map<string, { round: number; approved: boolean }>();
  for (const question of item.questions ?? []) {
    if (question.kind !== "review" || question.state !== "answered" || !question.review) continue;
    const { piece, round } = question.review;
    const known = latest.get(piece);
    if (known && known.round >= round) continue;
    latest.set(piece, { round, approved: question.reply?.picked[0] === 0 });
  }
  const pieces = latest.size;
  const approved = [...latest.values()].filter((entry) => entry.approved).length;
  if (pieces === 0) parts.push("no reviews");
  else if (pieces === 1) parts.push(approved === 1 ? "review approved" : "review not approved");
  else if (pieces === 2 && approved === 2) parts.push("reviews: 2, both approved");
  else if (approved === pieces) parts.push(`reviews: ${pieces}, all approved`);
  else parts.push(`reviews: ${pieces}, ${approved} approved`);
  return parts.join(" · ");
}
