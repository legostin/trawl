import { FolderTree, List } from "lucide-react";
import { useFlows, type ListMode } from "../store";
import { Segmented } from "./ui/segmented";
import { TrafficTable } from "./TrafficTable";
import { StructureTree } from "./StructureTree";

export function ListPanel() {
  const listMode = useFlows((s) => s.listMode);
  const setListMode = useFlows((s) => s.setListMode);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center border-b border-border bg-card px-2 py-1">
        <Segmented<ListMode>
          value={listMode}
          onChange={setListMode}
          options={[
            {
              value: "sequence",
              label: (
                <>
                  <List className="size-3.5" />
                  Sequence
                </>
              ),
            },
            {
              value: "structure",
              label: (
                <>
                  <FolderTree className="size-3.5" />
                  Structure
                </>
              ),
            },
          ]}
        />
      </div>
      <div className="flex min-h-0 flex-1 flex-col">
        {listMode === "structure" ? <StructureTree /> : <TrafficTable />}
      </div>
    </div>
  );
}
