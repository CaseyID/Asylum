import type { CSSProperties, JSX } from "react";
import {
  Activity,
  Archive,
  ArrowLeft,
  ArrowRight,
  Bell,
  BookOpen,
  ChevronRight,
  Circle,
  Copy,
  Download,
  Edit2,
  ExternalLink,
  Filter,
  GitBranch,
  GitPullRequest,
  LayoutGrid,
  List,
  Maximize2,
  MessageSquare,
  Moon,
  MoreHorizontal,
  Paperclip,
  Play,
  Plus,
  RotateCcw,
  Rss,
  Save,
  Search,
  Send,
  Settings,
  Square,
  Sun,
  Terminal,
  ThumbsUp,
  Trash2,
  Upload,
  Wrench,
  X,
  Zap,
  type LucideIcon,
} from "lucide-react";

const ICONS: Record<string, LucideIcon> = {
  activity: Activity,
  archive: Archive,
  "arrow-left": ArrowLeft,
  "arrow-right": ArrowRight,
  bell: Bell,
  "book-open": BookOpen,
  "chevron-right": ChevronRight,
  circle: Circle,
  copy: Copy,
  download: Download,
  "edit-2": Edit2,
  "external-link": ExternalLink,
  filter: Filter,
  "git-branch": GitBranch,
  "git-pull-request": GitPullRequest,
  "layout-grid": LayoutGrid,
  list: List,
  "maximize-2": Maximize2,
  "message-square": MessageSquare,
  moon: Moon,
  "more-horizontal": MoreHorizontal,
  paperclip: Paperclip,
  play: Play,
  plus: Plus,
  "rotate-ccw": RotateCcw,
  rss: Rss,
  save: Save,
  search: Search,
  send: Send,
  settings: Settings,
  square: Square,
  sun: Sun,
  terminal: Terminal,
  "thumbs-up": ThumbsUp,
  "trash-2": Trash2,
  upload: Upload,
  wrench: Wrench,
  x: X,
  zap: Zap,
};

interface IconProps {
  name: string;
  size?: number;
  style?: CSSProperties;
  className?: string;
}

export function Icon({ name, size = 16, style, className }: IconProps): JSX.Element {
  const Cmp = ICONS[name];
  if (!Cmp) {
    return (
      <span
        className={`ico ${className ?? ""}`}
        aria-hidden="true"
        style={{ display: "inline-block", width: size, height: size, ...style }}
      />
    );
  }
  return (
    <Cmp
      className={`ico ${className ?? ""}`}
      size={size}
      strokeWidth={2}
      style={{ display: "inline-block", flexShrink: 0, ...style }}
      aria-hidden="true"
    />
  );
}
