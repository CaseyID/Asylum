import { useState, type JSX } from "react";
import { Icon } from "../lib/icons";

export interface ToastPayload {
  id: string;
  /** free-form display string (e.g. "ntfy:user@host") */
  from: string;
  /** node id to target for replies; null when the inbound message is not routed. */
  nodeId: string | null;
  channel: string;
  subject?: string;
  body: string;
  replies: string[];
}

export interface NtfyToastProps {
  toast: ToastPayload;
  onDismiss: () => void;
  onReply: (text: string) => void;
}

export function NtfyToast({ toast, onDismiss, onReply }: NtfyToastProps): JSX.Element {
  const [reply, setReply] = useState("");

  function sendReply(text: string) {
    onReply(text);
    onDismiss();
  }

  return (
    <div className="toast">
      <div className="h">
        <Icon name="bell" size={12} />
        <span>ntfy</span>
        <span className="ch" style={{ opacity: 0.7 }}>
          · {toast.channel}
        </span>
        <span className="x" onClick={onDismiss}>
          <Icon name="x" size={12} />
        </span>
      </div>
      <div className="body">
        <div className="from">{toast.from} → you</div>
        <div>{toast.body}</div>
      </div>
      {toast.nodeId === null ? (
        <div className="reply-unavailable" style={{ opacity: 0.5, fontSize: "0.8em", padding: "4px 0" }}>
          reply not available — message has no node target
        </div>
      ) : (
        <>
          <div className="quick">
            {toast.replies.map((r) => (
              <button key={r} className="q" onClick={() => sendReply(r)}>
                {r}
              </button>
            ))}
          </div>
          <div className="reply">
            <span className="glyph">{">"}</span>
            <input
              placeholder="reply to send command…"
              value={reply}
              onChange={(e) => setReply(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && reply.trim()) sendReply(reply);
              }}
            />
            <button
              className="send"
              onClick={() => {
                if (reply.trim()) sendReply(reply);
              }}
            >
              send ↵
            </button>
          </div>
        </>
      )}
    </div>
  );
}
