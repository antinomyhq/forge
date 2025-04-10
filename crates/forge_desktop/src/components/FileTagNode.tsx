import { Node, mergeAttributes, Extension } from "@tiptap/core";
import { ReactNodeViewRenderer, NodeViewWrapper } from "@tiptap/react";
import { X } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export interface FileTagAttrs {
  filePath: string;
  id: string;
}

// Helper function to get basename from a path
const getBasename = (filepath: string | null | undefined): string => {
  if (!filepath) {
    return "Unknown";
  }
  const normalizedPath = filepath.replace(/\\/g, "/");
  const parts = normalizedPath.split("/");
  return parts[parts.length - 1] || filepath;
};

// React component for rendering the FileTag
interface FileTagComponentProps {
  node: { attrs: FileTagAttrs };
  deleteNode: () => void;
}

const FileTagComponent: React.FC<FileTagComponentProps> = (props) => {
  const { filePath } = props.node.attrs;
  const fileName = getBasename(filePath);

  return (
    <NodeViewWrapper as="span">
      <TooltipProvider>
        <Tooltip delayDuration={300}>
          <TooltipTrigger asChild>
            <span // Using span instead of div to fit better inline
              contentEditable={false} // Important - prevents the node from being edited
              className="inline-flex items-center bg-blue-50 border border-blue-100 rounded-md py-0.5 px-1.5 mr-1 my-0 mx-0.5 text-xs whitespace-nowrap hover:bg-blue-100 transition-colors"
              data-file-tag
            >
              <span className="truncate max-w-[120px] text-blue-700 font-medium">
                {fileName}
              </span>
              <button
                onClick={(e: React.MouseEvent<HTMLButtonElement>) => {
                  e.preventDefault();
                  props.deleteNode();
                }}
                className="ml-1 text-muted-foreground hover:text-destructive transition-colors"
                aria-label="Remove file tag"
              >
                <X className="h-3 w-3" />
              </button>
            </span>
          </TooltipTrigger>
          <TooltipContent side="top">
            <p className="max-w-[300px] break-all text-xs">
              {filePath || "Unknown path"}
            </p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </NodeViewWrapper>
  );
};

// Custom extension to detect and parse file tag syntax
export const FileTagPasteHandler = Extension.create({
  name: "fileTagPasteHandler",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("fileTagPasteHandler"),
        appendTransaction: (transactions, _oldState, newState) => {
          // Skip if no content changes
          if (!transactions.some((tr) => tr.docChanged)) {
            return null;
          }

          // Get the document's text content
          const text = newState.doc.textContent;

          // Create a transaction to modify the document
          const tr = newState.tr;
          let hasChanges = false;

          // Look for file tag syntax patterns
          const fileTagRegex = /@\[(.*?)\]/g;
          let match;

          // Collect matches to avoid modifying while iterating
          const matches: Array<{
            filePath: string;
            startPos: number;
            endPos: number;
          }> = [];

          while ((match = fileTagRegex.exec(text)) !== null) {
            const filePath = match[1];
            if (filePath) {
              // Find the exact position in the document
              let foundPos = false;
              let startPos = 0;

              // Find exact position in document
              newState.doc.descendants((node, pos) => {
                if (foundPos) return false;

                if (node.isText && node.text) {
                  const tagStart = node.text.indexOf("@[" + filePath + "]");
                  if (tagStart !== -1) {
                    startPos = pos + tagStart;
                    foundPos = true;
                    return false;
                  }
                }
                return true;
              });

              if (foundPos) {
                matches.push({
                  filePath,
                  startPos,
                  endPos: startPos + match[0].length,
                });
              }
            }
          }

          // Process matches in reverse to avoid position shifts
          matches.reverse().forEach(({ filePath, startPos, endPos }) => {
            // Delete the text version of the tag
            tr.delete(startPos, endPos);

            // Insert a file tag node
            const fileTagNode = newState.schema.nodes.fileTag.create({
              filePath,
              id: `file-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
            });

            tr.insert(startPos, fileTagNode);
            hasChanges = true;
          });

          return hasChanges ? tr : null;
        },
      }),
    ];
  },
});

// Define additional options for FileTagNode
interface FileTagNodeOptions {
  onRemove?: (filePath: string) => void;
}

// TipTap Node definition
const FileTagNode = Node.create<FileTagNodeOptions>({
  name: "fileTag",

  group: "inline",

  inline: true,

  selectable: false,

  atom: true, // Treat the node as a single unit

  addAttributes() {
    return {
      filePath: {
        default: null,
      },
      id: {
        default: null,
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-file-tag]",
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { "data-file-tag": "" }),
      0,
    ];
  },

  addNodeView() {
    return ReactNodeViewRenderer((props: any) => {
      return (
        <FileTagComponent
          node={props.node}
          deleteNode={() => {
            props.deleteNode();
            // If onRemove callback is provided, call it with the file path
            if (this.options.onRemove && props.node.attrs.filePath) {
              this.options.onRemove(props.node.attrs.filePath);
            }
          }}
        />
      );
    });
  },
});

export default FileTagNode;
