import { library } from "@/api/library"
import { useQuery } from "@tanstack/react-query"
import type { Bookshelf } from "@/lib/types"


export function useLibraryController() {
    const {
      data: shelves = [],
    } = useQuery<Bookshelf[]>({
      queryKey: ["bookshelves"],
      queryFn: () => library.getBookshelves(),
    })

    return { shelves }
}

export type LibraryController = ReturnType<typeof useLibraryController>
