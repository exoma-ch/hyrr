/** Global bug report state — accessible from any component. */

let open = $state(false);

export function getBugReportOpen(): boolean {
  return open;
}

export function openBugReport(): void {
  open = true;
}

export function closeBugReport(): void {
  open = false;
}
