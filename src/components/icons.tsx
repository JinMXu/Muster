// App icon set, powered by Lucide (lucide-react): professionally designed on
// a consistent 24×24 grid with uniform stroke weight, so every glyph reads
// as one family at the small sizes this app uses (10–15px).
//
// Wrappers keep the historical props API (`size?: number` + any SVG attr):
// 14px default size, strokeWidth 2 (Lucide's canonical weight — 1.5 washed
// out below ~13px), currentColor (colors always inherit from the
// surrounding text color — never hardcoded). `IconRestore` is the one
// hand-drawn exception: the Windows caption "restore" glyph (two
// overlapping squares) has no Lucide equivalent.
import type { SVGProps } from "react";
import {
  ArrowUpRight,
  ChartColumn,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Columns2,
  File,
  Folder,
  FolderPlus,
  GitBranch,
  Globe,
  Info,
  Keyboard,
  Maximize2,
  MessageCircle,
  Minus,
  PanelRight,
  Pencil,
  Plug,
  Plus,
  RefreshCw,
  Search,
  Settings,
  Square,
  Terminal,
  Trash,
  Type,
  X,
  type LucideIcon,
} from "lucide-react";

export type IconProps = SVGProps<SVGSVGElement> & {
  /// Icon edge length in px (icons are drawn on a 24×24 grid).
  size?: number;
};

function fromLucide(Icon: LucideIcon) {
  return function MusterIcon({ size = 14, ...props }: IconProps) {
    return <Icon aria-hidden="true" {...props} size={size} strokeWidth={2} />;
  };
}

export const IconInfo = fromLucide(Info);
export const IconFolder = fromLucide(Folder);
export const IconFolderPlus = fromLucide(FolderPlus);
export const IconFile = fromLucide(File);
export const IconGitBranch = fromLucide(GitBranch);
export const IconPlus = fromLucide(Plus);
export const IconMinus = fromLucide(Minus);
export const IconSettings = fromLucide(Settings);
export const IconMessageCircle = fromLucide(MessageCircle);
export const IconX = fromLucide(X);
export const IconSearch = fromLucide(Search);
export const IconChevronRight = fromLucide(ChevronRight);
export const IconChevronDown = fromLucide(ChevronDown);
export const IconChevronUp = fromLucide(ChevronUp);
export const IconChevronLeft = fromLucide(ChevronLeft);
export const IconPencil = fromLucide(Pencil);
export const IconTrash = fromLucide(Trash);
export const IconMaximize2 = fromLucide(Maximize2);
export const IconPanelRight = fromLucide(PanelRight);
export const IconSquare = fromLucide(Square);
export const IconRefresh = fromLucide(RefreshCw);
export const IconArrowUpRight = fromLucide(ArrowUpRight);
export const IconChartBar = fromLucide(ChartColumn);
export const IconTerminal = fromLucide(Terminal);
export const IconKeyboard = fromLucide(Keyboard);
export const IconType = fromLucide(Type);
export const IconPlug = fromLucide(Plug);
export const IconGlobe = fromLucide(Globe);
export const IconCheck = fromLucide(Check);
export const IconColumns = fromLucide(Columns2);

/// Windows-style restore glyph: two overlapping squares.
export function IconRestore(props: IconProps) {
  const { size = 14, ...rest } = props;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      <path d="M15 9V5H5v10h4" />
      <rect x="9" y="9" width="10" height="10" rx="1" />
    </svg>
  );
}
