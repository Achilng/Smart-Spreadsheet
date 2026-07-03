import type { RowRecord } from "../api";

export interface SectionMembers {
  rows: RowRecord[];
  totalCount: number;
  loading: boolean;
  error: string | null;
}
