import { type FC } from "react";
import { type AsylumNode } from "../api";

export interface NodeTableProps {
  nodes: AsylumNode[];
  selectedNodeId?: string;
  onSelectNode: (id: string) => void;
}

export const NodeTable: FC<NodeTableProps> = ({ nodes, selectedNodeId, onSelectNode }) => {
  return (
    <section className="panel">
      <h3>Node Table</h3>
      <div className="table-shell">
        <table className="node-table">
          <thead>
            <tr>
              <th>Role</th>
              <th>Harness</th>
              <th>Substrate</th>
              <th>State</th>
              <th>Output</th>
            </tr>
          </thead>
          <tbody>
            {nodes.length === 0 ? (
              <tr>
                <td colSpan={5} className="empty-cell">
                  No nodes yet. Create one to begin.
                </td>
              </tr>
            ) : (
              nodes.map((node) => (
                <tr
                  key={node.id}
                  className={selectedNodeId === node.id ? "selected-row" : ""}
                  onClick={() => onSelectNode(node.id)}
                >
                  <td>{node.role_hint}</td>
                  <td>{node.harness}</td>
                  <td>{node.substrate}</td>
                  <td>{node.liveness}</td>
                  <td className="output-cell">{node.output_preview ?? "—"}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
};
