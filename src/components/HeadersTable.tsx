import type { Header } from "@/types";

export function HeadersTable({ headers }: { headers: Header[] }) {
  if (headers.length === 0) {
    return <div className="p-3 text-xs text-muted-foreground">Нет заголовков</div>;
  }
  return (
    <table className="w-full border-collapse text-xs">
      <tbody>
        {headers.map(([k, v], i) => (
          <tr key={i} className="border-b border-border/50 align-top">
            <td className="w-1/3 py-1 pr-3 font-mono font-medium text-muted-foreground break-words">
              {k}
            </td>
            <td className="py-1 font-mono break-words text-foreground">{v}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
