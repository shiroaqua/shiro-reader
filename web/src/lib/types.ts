export type UUID = string;


export interface Book {
    id: UUID;
    title: string;
    hash: string;
    type: string;
    created_at: string;
    updated_at: string;
  }

export interface CreateBookResponse {
    id: UUID;
    created_at: string;
  }
