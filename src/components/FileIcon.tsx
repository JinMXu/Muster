import { Icon, iconLoaded } from "@iconify/react";
import { fileIconName, useIconsReady } from "../lib/fileIcons";
import { IconFile } from "./icons";

/// File-type icon for a tree row. Falls back to the app's generic file icon
/// until the vscode-icons collection has loaded (and for names it doesn't
/// cover), so nothing here ever waits on — or hits — the network.
export default function FileIcon({ name, size = 12 }: { name: string; size?: number }) {
  const ready = useIconsReady();
  const icon = fileIconName(name);
  if (!ready || !iconLoaded(icon)) {
    return <IconFile size={size} />;
  }
  return <Icon icon={icon} width={size} height={size} aria-hidden />;
}
