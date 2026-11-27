export type UUID = string;


export interface Book {
    id: UUID;
    title: string;
    hash: string;
    type: string;
    created_at: string;
    updated_at: string;
  }

export interface BookMetadata {
  book_id: UUID;
  subtitle: string | null;
  authors: string[];
  publisher: string | null;
  published_date: string | null;
  language: string | null;
  description: string | null;
  identifier: string | null;
  type: string;
  size: number;
  page_count: number | null;
  created_at: string;
  updated_at: string;
}

export interface TrashedBook extends Book {
  deleted_at: string;
}

export type TagValueType =
  | "none"
  | "number"
  | "text"
  | "date"
  | "language"
  | "issn_isbn"
  | "clc"
  | "ddc"
  | "lcc";

export interface Tag {
  id: UUID;
  name: string;
  parent_id: UUID | null;
  color: string;
  value_type: TagValueType;
  value_regex: string | null;
  created_at: string;
  updated_at: string;
}

export type TagDefinitionInput = Omit<
  Tag,
  "id" | "value_regex" | "created_at" | "updated_at"
>;

export interface TagTreeNode extends Tag {
  children: TagTreeNode[];
}

export interface BookTag {
  tag_id: UUID;
  value: string | null;
}

export type UndefineTagStrategy = "CASCADE" | "PROMOTE";
