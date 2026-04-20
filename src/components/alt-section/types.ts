import type { ModRow } from "../../lib/types";

export type AltTLItem =
  | { kind: "alt-group"; id: string; name: string; collapsed: boolean; blocks: ModRow[]; blockIds: string[] }
  | { kind: "alt-row"; row: ModRow };

export interface AltSectionProps {
  parentRow: ModRow;
  depth: number;
  onReorderAlts?: (parentId: string, orderedIds: string[]) => void;
}

export const altTlId = (item: AltTLItem): string =>
  item.kind === "alt-row" ? item.row.id : `alt-group:${item.id}`;
