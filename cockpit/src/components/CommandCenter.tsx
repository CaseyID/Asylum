import { TerminalSquare, Send } from "lucide-react";
import { type FC, useEffect, useMemo, useRef, useState } from "react";
import { type AsylumNode, postNodeInput } from "../api";
import { selectCommandCenter } from "../state";

export interface CommandCenterProps {
  nodes: AsylumNode[];
  selectedNodeId?: string;
  onSelectNode: (id: string) => void;
}

interface ChatLine {
  source: "user" | "node";
  text: string;
  at: string;
}

export const CommandCenter: FC<CommandCenterProps> = ({ nodes, selectedNodeId, onSelectNode }) => {
  const commandCenter = useMemo(() => {
    if (selectedNodeId) {
      const node = nodes.find((item) => item.id === selectedNodeId);
      if (node?.role_hint === "command-center") {
        return node;
      }
    }
    const selected = selectCommandCenter(nodes);
    return nodes.find((node) => node.id === selected?.id);
  }, [nodes, selectedNodeId]);

  useEffect(() => {
    if (commandCenter && commandCenter.id !== selectedNodeId) {
      onSelectNode(commandCenter.id);
    }
  }, [commandCenter, onSelectNode, selectedNodeId]);

  const [message, setMessage] = useState("");
  const [lines, setLines] = useState<ChatLine[]>([]);
  const socketRef = useRef<WebSocket | null>(null);
  const logRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!commandCenter) {
      return;
    }
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const socket = new WebSocket(`${protocol}://${window.location.host}/api/nodes/${commandCenter.id}/observe/ws`);
    socketRef.current = socket;

    socket.addEventListener("message", (event) => {
      const messageLine: ChatLine = {
        source: "node",
        text: String(event.data),
        at: new Date().toISOString(),
      };
      setLines((previous) => [...previous, messageLine].slice(-250));
    });
    socket.addEventListener("close", () => {
      socketRef.current = null;
    });

    return () => {
      socket.close();
      if (socketRef.current === socket) {
        socketRef.current = null;
      }
    };
  }, [commandCenter]);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight, behavior: "smooth" });
  }, [lines]);

  const send = async () => {
    if (!commandCenter) return;
    const trimmed = message.trim();
    if (!trimmed) return;

    const outgoing: ChatLine = {
      source: "user",
      text: trimmed,
      at: new Date().toISOString(),
    };
    setLines((previous) => [...previous, outgoing].slice(-250));
    setMessage("");
    try {
      await postNodeInput(commandCenter.id, trimmed);
      const delivered: ChatLine = {
        source: "node",
        text: "Input delivered.",
        at: new Date().toISOString(),
      };
      setLines((previous) => [...previous, delivered].slice(-250));
    } catch (err) {
      const failure: ChatLine = {
        source: "node",
        text: `Input failed: ${String(err instanceof Error ? err.message : err)}`,
        at: new Date().toISOString(),
      };
      setLines((previous) => [...previous, failure].slice(-250));
    }
  };

  return (
    <section className="panel command-center">
      <h3>
        <TerminalSquare size={14} />
        {commandCenter ? `${commandCenter.harness} command for ${commandCenter.role_hint}` : "Command Center"}
      </h3>
      {!commandCenter && <p className="empty-cell">No running command-center. Launch one in Create Node controls.</p>}
      {commandCenter && (
        <>
          <div className="log" ref={logRef}>
            {lines.map((line) => (
              <p key={`${line.at}-${line.source}`} className={line.source === "user" ? "user-line" : "node-line"}>
                <span>{line.source}</span>
                <strong>{new Date(line.at).toLocaleTimeString()}</strong>
                {line.text}
              </p>
            ))}
          </div>
          <div className="inline-action-row">
            <input
              value={message}
              onChange={(event) => setMessage(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void send();
                }
              }}
              placeholder={`Type to ${commandCenter.harness}/${commandCenter.role_hint}...`}
            />
            <button type="button" onClick={send}>
              <Send size={14} /> Send
            </button>
          </div>
          <p className="micro">
            Backend endpoint: <code>/api/nodes/{commandCenter.id}/input</code>
          </p>
        </>
      )}
    </section>
  );
};
