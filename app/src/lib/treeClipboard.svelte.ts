// In-app clipboard of the navigation tree (cut / copy / paste of notes and
// folders). Lives outside the panel so it survives closing and reopening it.

export type TreeKind = "note" | "folder";

export interface TreeClip {
  mode: "cut" | "copy";
  kind: TreeKind;
  id: string;
}

export const treeClipboard = $state<{ clip: TreeClip | null }>({ clip: null });
