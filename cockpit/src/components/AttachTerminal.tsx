import { type FC, useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

export interface AttachTerminalProps {
  token: string;
  onClose: () => void;
}

export const AttachTerminal: FC<AttachTerminalProps> = ({ token, onClose }) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const socketRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const terminal = new Terminal({
      cursorBlink: true,
      rows: 18,
      convertEol: true,
      fontSize: 13,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(containerRef.current);
    fitAddon.fit();
    terminal.write("\r\nConnecting terminal...\r\n");

    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const socket = new WebSocket(`${protocol}://${window.location.host}/api/attach/${token}/ws`);
    socketRef.current = socket;

    socket.addEventListener("open", () => {
      terminal.writeln("Connected.");
    });

    socket.addEventListener("message", (event) => {
      terminal.write(String(event.data));
    });

    socket.addEventListener("close", () => {
      terminal.writeln("\r\nConnection closed.");
    });

    socket.addEventListener("error", () => {
      terminal.writeln("\r\nTerminal attach socket error.");
    });

    terminal.onData((data) => {
      socket.send(data);
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
    });
    resizeObserver.observe(containerRef.current);

    terminalRef.current = terminal;

    return () => {
      resizeObserver.disconnect();
      terminal.dispose();
      socket.close();
      terminalRef.current = null;
      socketRef.current = null;
    };
  }, [token]);

  return (
    <section className="panel terminal-panel">
      <div className="panel-header">
        <h4>Browser Terminal</h4>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
      <div ref={containerRef} className="terminal-shell" />
    </section>
  );
};
