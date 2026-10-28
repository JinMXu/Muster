// Generated from monaco-editor 0.5x esm/vs/editor/editor.main.js with all
// language tokenizers (languages/definitions/*), the lsp-client language
// features (languages/features/*), the classic language services
// (language/*) and the monaco-lsp-client external stripped out.
//
// editor.api alone does not wire any editor contributions (find, folding,
// context menu, bracket matching…); monaco.ts imports this module right
// after it so the editor keeps every editing feature while skipping the
// ~90 unused languages. The four language services the app actually uses
// are imported explicitly in monaco.ts.
//
// Regenerate (after a monaco-editor upgrade) with the node one-liner in
// git history, or diff against editor.main.js imports.
import "monaco-editor/editor/contrib/anchorSelect/browser/anchorSelect";
import "monaco-editor/editor/contrib/bracketMatching/browser/bracketMatching";
import "monaco-editor/editor/contrib/caretOperations/browser/transpose";
import "monaco-editor/editor/contrib/clipboard/browser/clipboard";
import "monaco-editor/editor/contrib/codeAction/browser/codeActionContributions";
import "monaco-editor/editor/browser/widget/codeEditor/codeEditorWidget";
import "monaco-editor/editor/contrib/codelens/browser/codelensController";
import "monaco-editor/base/browser/ui/codicons/codicon/codicon.css";
import "monaco-editor/editor/contrib/colorPicker/browser/colorPickerContribution";
import "monaco-editor/editor/contrib/comment/browser/comment";
import "monaco-editor/editor/contrib/contextmenu/browser/contextmenu";
import "monaco-editor/editor/contrib/cursorUndo/browser/cursorUndo";
import "monaco-editor/editor/browser/widget/diffEditor/diffEditor.contribution";
import "monaco-editor/editor/contrib/diffEditorBreadcrumbs/browser/contribution";
import "monaco-editor/editor/contrib/dnd/browser/dnd";
import "monaco-editor/editor/contrib/documentSymbols/browser/documentSymbols";
import "monaco-editor/editor/contrib/dropOrPasteInto/browser/dropIntoEditorContribution";
import "monaco-editor/features/find/register";
import "monaco-editor/editor/contrib/floatingMenu/browser/floatingMenu.contribution";
import "monaco-editor/editor/contrib/folding/browser/folding";
import "monaco-editor/editor/contrib/fontZoom/browser/fontZoom";
import "monaco-editor/editor/contrib/format/browser/formatActions";
import "monaco-editor/editor/contrib/gotoError/browser/gotoError";
import "monaco-editor/editor/standalone/browser/quickAccess/standaloneGotoLineQuickAccess";
import "monaco-editor/editor/contrib/gotoSymbol/browser/link/goToDefinitionAtPosition";
import "monaco-editor/editor/contrib/gpu/browser/gpuActions";
import "monaco-editor/editor/contrib/hover/browser/hoverContribution";
import "monaco-editor/editor/contrib/indentation/browser/indentation";
import "monaco-editor/editor/contrib/inlayHints/browser/inlayHintsContribution";
import "monaco-editor/editor/contrib/inlineCompletions/browser/inlineCompletions.contribution";
import "monaco-editor/editor/contrib/inlineProgress/browser/inlineProgress";
import "monaco-editor/editor/contrib/inPlaceReplace/browser/inPlaceReplace";
import "monaco-editor/editor/contrib/insertFinalNewLine/browser/insertFinalNewLine";
import "monaco-editor/editor/standalone/browser/inspectTokens/inspectTokens";
import "monaco-editor/editor/standalone/browser/iPadShowKeyboard/iPadShowKeyboard";
import "monaco-editor/editor/contrib/lineSelection/browser/lineSelection";
import "monaco-editor/editor/contrib/linesOperations/browser/linesOperations";
import "monaco-editor/editor/contrib/linkedEditing/browser/linkedEditing";
import "monaco-editor/editor/contrib/links/browser/links";
import "monaco-editor/editor/contrib/longLinesHelper/browser/longLinesHelper";
import "monaco-editor/editor/contrib/middleScroll/browser/middleScroll.contribution";
import "monaco-editor/editor/contrib/multicursor/browser/multicursor";
import "monaco-editor/editor/contrib/parameterHints/browser/parameterHints";
import "monaco-editor/editor/contrib/placeholderText/browser/placeholderText.contribution";
import "monaco-editor/editor/standalone/browser/quickAccess/standaloneCommandsQuickAccess";
import "monaco-editor/editor/standalone/browser/quickAccess/standaloneHelpQuickAccess";
import "monaco-editor/editor/standalone/browser/quickAccess/standaloneGotoSymbolQuickAccess";
import "monaco-editor/editor/contrib/readOnlyMessage/browser/contribution";
import "monaco-editor/editor/standalone/browser/referenceSearch/standaloneReferenceSearch";
import "monaco-editor/editor/contrib/rename/browser/rename";
import "monaco-editor/editor/contrib/sectionHeaders/browser/sectionHeaders";
import "monaco-editor/editor/contrib/semanticTokens/browser/viewportSemanticTokens";
import "monaco-editor/editor/contrib/smartSelect/browser/smartSelect";
import "monaco-editor/editor/contrib/snippet/browser/snippetController2";
import "monaco-editor/editor/contrib/stickyScroll/browser/stickyScrollContribution";
import "monaco-editor/editor/contrib/suggest/browser/suggestInlineCompletions";
import "monaco-editor/editor/standalone/browser/toggleHighContrast/toggleHighContrast";
import "monaco-editor/editor/contrib/toggleTabFocusMode/browser/toggleTabFocusMode";
import "monaco-editor/editor/contrib/tokenization/browser/tokenization";
import "monaco-editor/editor/contrib/unicodeHighlighter/browser/unicodeHighlighter";
import "monaco-editor/editor/contrib/unusualLineTerminators/browser/unusualLineTerminators";
import "monaco-editor/editor/contrib/wordHighlighter/browser/wordHighlighter";
import "monaco-editor/editor/contrib/wordOperations/browser/wordOperations";
import "monaco-editor/editor/contrib/wordPartOperations/browser/wordPartOperations";
import "monaco-editor/editor/browser/coreCommands";
import "monaco-editor/editor/contrib/caretOperations/browser/caretOperations";
import "monaco-editor/editor/contrib/dropOrPasteInto/browser/copyPasteContribution";
import "monaco-editor/editor/contrib/find/browser/findController";
import "monaco-editor/editor/contrib/gotoSymbol/browser/goToCommands";
import "monaco-editor/editor/contrib/gotoError/browser/markerSelectionStatus";
import "monaco-editor/editor/contrib/semanticTokens/browser/documentSemanticTokens";
import "monaco-editor/editor/contrib/suggest/browser/suggestController";
import "monaco-editor/editor/common/standaloneStrings";
import "monaco-editor/base/browser/ui/codicons/codicon/codicon-modifiers.css";
