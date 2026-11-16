export type UUID = string;


export interface Book {
    id: UUID;
    title: string;
    hash: string;
    type: string;
    created_at: string;
    updated_at: string;
  }

export interface Tag {
  id: UUID;
  name: string;
  parent_id: UUID | null;
  color: string;
  allows_value: boolean;
  value_regex: string | null;
}

export interface TagTreeNode extends Tag {
  children: TagTreeNode[];
}

export interface BookTag {
  tag_id: UUID;
  value: string | null;
}

export type UndefineTagStrategy = "CASCADE" | "PROMOTE";
