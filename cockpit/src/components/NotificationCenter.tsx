import { Bell } from "lucide-react";
import { type FC, useEffect, useMemo, useState } from "react";
import { type NotificationRecord, fetchNotifications, markNotificationRead } from "../api";

export interface NotificationCenterProps {
  notifications: NotificationRecord[];
  onRefresh: () => Promise<void>;
}

export const NotificationCenter: FC<NotificationCenterProps> = ({ notifications, onRefresh }) => {
  const [busy, setBusy] = useState(false);
  const critical = useMemo(() => notifications.filter((item) => item.severity === "warn" || item.severity === "error"), [notifications]);

  useEffect(() => {
    const timer = setInterval(() => {
      if (!busy) {
        void onRefresh();
      }
    }, 8000);
    return () => clearInterval(timer);
  }, [busy, onRefresh]);

  const markRead = async (id: string) => {
    try {
      setBusy(true);
      await markNotificationRead(id);
      await onRefresh();
    } catch {
      // ignored; backend may not expose read endpoint yet.
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="notification-center panel">
      <h3>
        <Bell size={14} /> Notifications
      </h3>
      <div className="notification-body">
        {critical.length > 0 && <div className="warning">{critical.length} item(s) need attention.</div>}
        {notifications.length === 0 ? (
          <p className="empty-cell">No notifications.</p>
        ) : (
          <ul>
            {notifications.slice(0, 12).map((note) => (
              <li key={note.id} className={note.read ? "read" : "unread"}>
                <div>
                  <strong>{note.title || "System event"}</strong> — {note.body}
                </div>
                <div className="mini-row">
                  <span>{new Date(note.created_at).toLocaleTimeString()}</span>
                  {!note.read && (
                    <button type="button" onClick={() => markRead(note.id)} disabled={busy}>
                      mark read
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
      <button type="button" className="ghost-btn" onClick={() => void onRefresh()} disabled={busy}>
        Reload
      </button>
    </section>
  );
};
