export type UUID = string;


export interface Bookshelf {
    id: UUID;
    name: string;
    created_at: string;
    updated_at: string;
  }

export interface Book {
    id: UUID;
    title: string;
    hash: string;
    type: string;
    bookshelf_id: UUID;
    created_at: string;
    updated_at: string;
  }
  
export interface CreateBookResponse {
    id: UUID;
    created_at: string;
  }
