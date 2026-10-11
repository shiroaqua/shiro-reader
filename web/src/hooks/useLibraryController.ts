import { library } from "@/api/library"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { bookshelfBooksQueryKey } from "@/lib/query-keys"
import type { Book, Bookshelf } from "@/lib/types"
import { useState } from "react";


export function useLibraryController() {
    const queryClient = useQueryClient()
    const {
      data: shelves = [],
      refetch
    } = useQuery<Bookshelf[]>({
      queryKey: ["bookshelves"],
      queryFn: () => library.bookshelf.list(),
    })

    const deleteBookshelfMutation = useMutation({
      mutationFn: (id: Bookshelf["id"]) => library.bookshelf.delete(id),
      onSuccess: async (_data, id) => {
        await queryClient.cancelQueries({
          queryKey: ["bookshelves"],
          exact: true,
        })
        queryClient.setQueryData<Bookshelf[]>(["bookshelves"], (current) =>
          current?.filter((shelf) => shelf.id !== id)
        )
        await queryClient.invalidateQueries({
          queryKey: ["bookshelves"],
          exact: true,
        })
      },
    })


    const renameBookshelfMutation = useMutation({
      mutationFn: ({ id, name }: { id: Bookshelf["id"]; name: string }) =>
        library.bookshelf.rename(id, name.trim()),
      onSuccess: async (_data, { id, name }) => {
        await queryClient.cancelQueries({
          queryKey: ["bookshelves"],
          exact: true,
        })
        queryClient.setQueryData<Bookshelf[]>(["bookshelves"], (current) =>
          current?.map((shelf) =>
            shelf.id === id ? { ...shelf, name: name.trim() } : shelf
          )
        )
        await queryClient.invalidateQueries({
          queryKey: ["bookshelves"],
          exact: true,
        })
      },
    })

    // Always refetches so a stale cache never lets a non-empty shelf skip confirmation.
    const isBookshelfEmpty = async (id: Bookshelf["id"]) => {
      const books = await queryClient.fetchQuery<Book[]>({
        queryKey: bookshelfBooksQueryKey(id),
        queryFn: () => library.book.list(id),
        staleTime: 0,
      })
      return books.length === 0
    }

    const [newBookshelfName, setNewBookshelfName] = useState('');
    const createBookshelf = async () => {
      const name = newBookshelfName.trim();
      if(!name)
        return;

      await library.bookshelf.create(name);
      setNewBookshelfName('');
      refetch();
    }
    return {
      shelves,
      newBookshelfName,
      createBookshelf,
      setNewBookshelfName,
      deleteBookshelf: deleteBookshelfMutation.mutateAsync,
      isBookshelfEmpty,
      isDeletingBookshelf: deleteBookshelfMutation.isPending,
      deletingBookshelfId: deleteBookshelfMutation.isPending
        ? deleteBookshelfMutation.variables ?? null
        : null,
      renameBookshelf: renameBookshelfMutation.mutateAsync,
      isRenamingBookshelf: renameBookshelfMutation.isPending,
    }
}

export type LibraryController = ReturnType<typeof useLibraryController>
