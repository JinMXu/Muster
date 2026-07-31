/// Shared modal overlay: centered card with backdrop, `muster-pop` animation,
/// and click-outside-to-close. Consistent visual style across Settings,
/// UsagePanel, PasteWarning, GitPanel discard confirm, and App close prompt.
export default function Modal({
  children,
  onClose,
  width = 440,
  labelledBy,
}: {
  children: React.ReactNode;
  onClose?: () => void;
  width?: number;
  labelledBy?: string;
}) {
  return (
    <div
      className="fixed inset-0 z-40 bg-black/35 flex items-center justify-center"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        className="bg-muster-bg border border-white/[0.08] rounded-[10px] shadow-[0_12px_32px_rgba(0,0,0,0.5)] px-5 py-4 muster-pop"
        style={{ width: `${width}px` }}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}