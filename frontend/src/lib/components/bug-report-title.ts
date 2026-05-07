export const TITLE_MAX_LEN = 70;

export function buildIssueTitle(
  reportType: "bug" | "feature",
  title: string,
  description: string,
): string {
  const prefix = reportType === "bug" ? "[Bug]" : "[Feature]";
  const userTitle = title.trim() || description.slice(0, TITLE_MAX_LEN);
  return `${prefix} ${userTitle.slice(0, TITLE_MAX_LEN)}`;
}

export function bodyTitleHeader(title: string, description: string): string {
  const t = title.trim() || description.slice(0, TITLE_MAX_LEN);
  return `## ${t}`;
}
