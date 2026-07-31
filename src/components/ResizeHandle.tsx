/// Thin vertical drag handle between a sidebar and the main area. Always
/// present in the layout (4px) so the panels on either side keep their own
/// backgrounds; highlights on hover and while dragging.
export default function ResizeHandle({ onMouseDown }: { onMouseDown: (e: React.MouseEvent) => void }) {
  return (
    <div
      onMouseDown={onMouseDown}
      className="w-1 flex-shrink-0 self-stretch cursor-col-resize bg-transparent hover:bg-muster-accent/30 active:bg-muster-accent/50 transition-colors duration-muster ease-muster"
    />
  );
}
