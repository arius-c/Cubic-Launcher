export type ContentVersionRule = { kind: string; mcVersions: string[]; loader: string };

export type ContentEntry = {
  id: string;
  source: string;
  versionRules: ContentVersionRule[];
};

export type ContentGroupData = {
  id: string;
  name: string;
  collapsed: boolean;
  entryIds: string[];
};

export type ContentMeta = {
  name: string;
  iconUrl?: string;
};

export type ContentTopLevelItem =
  | { kind: "entry"; entry: ContentEntry }
  | { kind: "group"; id: string; name: string; collapsed: boolean; entries: ContentEntry[] };

export interface ContentTabViewProps {
  type: string;
  modlistName: string;
  onAddContent: () => void;
}

export const CONTENT_TAB_LABELS: Record<string, string> = {
  resourcepack: "Resource Packs",
  datapack: "Data Packs",
  shader: "Shaders",
};
