import { library } from "@/api/library"
import { useQuery } from "@tanstack/react-query"
import type { Bookshelf } from "@/lib/types"
import { useState } from "react";


export function useLibraryController() {
    const {
      data: shelves = [],
      refetch
    } = useQuery<Bookshelf[]>({
      queryKey: ["bookshelves"],
      queryFn: () => library.bookshelf.list(),
    })


    const [newBookshelfName, setNewBookshelfName] = useState('');
    const createBookshelf = async () => {
      const name = newBookshelfName.trim();
      if(!name)
        return;

      await library.bookshelf.create(name);
      setNewBookshelfName('');
      refetch();
    }
    return { shelves, newBookshelfName, createBookshelf, setNewBookshelfName }
}

export type LibraryController = ReturnType<typeof useLibraryController>
