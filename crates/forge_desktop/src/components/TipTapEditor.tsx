import {
  useEffect,
  FormEvent,
  useState,
  forwardRef,
  useImperativeHandle,
} from "react";
import { useEditor, EditorContent, Extension, Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Node } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { FileTagAttrs } from "./FileTagNode";
import FileTagNode, { FileTagPasteHandler } from "./FileTagNode";
import { cn } from "@/utils/utils";

interface TipTapEditorProps {
  content: string;
  onChange: (text: string) => void;
  onSubmit: (e?: FormEvent) => void | Promise<void>;
  taggedFiles: string[];
  onRemoveFile: (index: number) => void;
  onFileDrop: (path: string) => void;
  disabled?: boolean;
  isDragging: boolean;
  placeholder?: string;
  className?: string;
  setTaggedFiles?: (files: string[]) => void;
}

interface SubmitExtensionOptions {
  onSubmit: () => void;
  removeLastTag: () => void;
}

// Create SubmitExtension to handle Enter for submission
const SubmitExtension = Extension.create<SubmitExtensionOptions>({
  name: "submitExtension",

  addProseMirrorPlugins() {
    const onSubmit = this.options.onSubmit;

    return [
      new Plugin({
        key: new PluginKey("submitExtension"),
        props: {
          handleKeyDown(_view, event) {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (onSubmit) {
                // Use a timeout to ensure the event completes before submission
                setTimeout(() => {
                  onSubmit();
                }, 10);
              }
              return true;
            }
            return false;
          },
        },
      }),
    ];
  },

  addOptions() {
    return {
      onSubmit: () => {},
      removeLastTag: () => {},
    };
  },
});

interface FileDropExtensionOptions {
  onFileDrop: (path: string) => void;
}

// Create extension to handle file dropping
const FileDropExtension = Extension.create<FileDropExtensionOptions>({
  name: "fileDrop",

  addProseMirrorPlugins() {
    const onFileDrop = this.options.onFileDrop;

    return [
      new Plugin({
        key: new PluginKey("fileDrop"),
        props: {
          handleDOMEvents: {
            dragover(_view, event) {
              // Make sure we can handle this drop
              if (
                event.dataTransfer &&
                event.dataTransfer.types.includes("text/plain")
              ) {
                event.preventDefault();
                return true;
              }
              return false;
            },
            drop(_, event) {
              // Stop propagation to allow for our own handling
              event.stopPropagation();

              if (
                event.dataTransfer &&
                event.dataTransfer.types.includes("text/plain")
              ) {
                const filePath = event.dataTransfer.getData("text/plain");
                if (filePath) {
                  onFileDrop(filePath);
                  event.preventDefault();
                  return true;
                }
              }
              return false;
            },
          },
        },
      }),
    ];
  },

  addOptions() {
    return {
      onFileDrop: () => {},
    };
  },
});

interface PasteHandlerExtensionOptions {
  onPaste: () => void;
}

// Create extension specifically for handling paste events
const PasteHandlerExtension = Extension.create<PasteHandlerExtensionOptions>({
  name: "pasteHandler",

  addProseMirrorPlugins() {
    const onPaste = this.options.onPaste;

    return [
      new Plugin({
        key: new PluginKey("pasteHandler"),
        props: {
          // Use handleDOMEvents instead of handlePaste to avoid interfering with cursor positioning
          handleDOMEvents: {
            paste(_view, _event) {
              if (onPaste) {
                // Call the paste handler after a small delay to ensure content is updated
                setTimeout(() => {
                  onPaste();
                }, 50);
              }
              return false; // Allow normal paste handling to continue
            },
          },
        },
      }),
    ];
  },

  addOptions() {
    return {
      onPaste: () => {},
    };
  },
});

const TipTapEditor = forwardRef<{ editor: Editor | null }, TipTapEditorProps>(
  (
    {
      content,
      onChange,
      onSubmit,
      taggedFiles,
      onRemoveFile,
      onFileDrop,
      disabled = false,
      isDragging,
      placeholder = "Type a message or drop files here...",
      className,
      setTaggedFiles,
    },
    ref,
  ) => {
    // State to track whether the editor has content
    const [hasContent, setHasContent] = useState(false);

    // Function to generate the message text that includes file tags in the right positions
    const generateFormattedMessage = (editor: Editor | null): string => {
      if (!editor) return "";

      let formattedMessage = "";
      const extractedTags: string[] = [];

      // Process each node in the document to build the message with file tags in correct positions
      editor.state.doc.descendants((node, _pos) => {
        if (node.type.name === "text") {
          // For text nodes, add their content directly
          formattedMessage += node.text;
        } else if (node.type.name === "fileTag" && node.attrs.filePath) {
          // For file tag nodes, add them in the @[path] format
          const filePath = node.attrs.filePath;
          formattedMessage += ` @[${filePath}] `;
          extractedTags.push(filePath);
        }
      });

      // Update the taggedFiles array for the store (for backward compatibility)
      if (setTaggedFiles) {
        setTaggedFiles(extractedTags);
      }

      return formattedMessage;
    };

    const editor = useEditor({
      extensions: [
        StarterKit.configure({
          heading: false,
          bulletList: false,
          orderedList: false,
          blockquote: false,
          codeBlock: false,
        }),
        FileTagPasteHandler,
        FileTagNode.configure({
          onRemove: () => {
            if (editor) {
              // When a tag is removed, update the formatted message and tag list
              const message = generateFormattedMessage(editor);
              onChange(message);
            }
          },
        }),
        PasteHandlerExtension.configure({
          onPaste: () => {
            if (editor) {
              // Update hasContent state for the placeholder
              setHasContent(!editor.isEmpty);

              // Generate formatted message that preserves file tag positions
              const message = generateFormattedMessage(editor);

              // Update parent component with the formatted message
              onChange(message);
            }
          },
        }),
        SubmitExtension.configure({
          onSubmit,
          removeLastTag: () => {
            if (taggedFiles.length > 0) {
              onRemoveFile(taggedFiles.length - 1);
            }
          },
        }),
        FileDropExtension.configure({
          onFileDrop,
        }),
      ],
      content,
      editable: !disabled,
      onCreate: ({ editor }) => {
        // Initialize state and handle initial content
        setHasContent(!editor.isEmpty);
        const initialMessage = generateFormattedMessage(editor);
        onChange(initialMessage);
      },
      onUpdate: ({ editor }) => {
        // Update hasContent state for placeholder visibility
        setHasContent(!editor.isEmpty);

        // Generate formatted message with file tags in the correct positions
        const formattedMessage = generateFormattedMessage(editor);

        // Update the message state in parent component
        onChange(formattedMessage);
      },
      editorProps: {
        attributes: {
          class: "outline-none",
        },
      },
    });

    // Expose the editor instance via the ref
    useImperativeHandle(ref, () => ({
      editor,
    }));

    // Helper function to compare arrays of strings
    const arraysEqual = (a: string[], b: string[]): boolean => {
      if (a.length !== b.length) return false;
      return a.every((val, index) => val === b[index]);
    };

    // Update the editor when taggedFiles changes from external sources
    useEffect(() => {
      if (!editor) return;

      // Extract all file tags currently in the document
      const extractTagsFromEditor = (): string[] => {
        const currentTags: string[] = [];
        editor.state.doc.descendants((node) => {
          if (node.type.name === "fileTag" && node.attrs.filePath) {
            currentTags.push(node.attrs.filePath);
          }
        });
        return currentTags;
      };

      // Compare current editor tags with the taggedFiles prop
      const currentTags = extractTagsFromEditor();

      // If editor tags don't match the store's tags, sync them
      if (!arraysEqual(currentTags, taggedFiles)) {
        // Find new tags to add (in taggedFiles but not in editor)
        const tagsToAdd = taggedFiles.filter(
          (file) => !currentTags.includes(file),
        );

        if (tagsToAdd.length > 0) {
          // Get the current cursor position
          const { selection } = editor.state;
          const pos = selection.empty ? selection.from : selection.from;

          // Insert each new tag at the cursor position
          tagsToAdd.forEach((tagPath) => {
            // Skip if tagPath is null or undefined
            if (!tagPath) return;

            const node = Node.fromJSON(editor.schema, {
              type: "fileTag",
              attrs: {
                filePath: tagPath,
                id: `file-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
              } as FileTagAttrs,
            });

            editor.commands.insertContentAt(pos, node);
          });

          // Update the message content after adding tags
          const formattedMessage = generateFormattedMessage(editor);
          onChange(formattedMessage);
        }
      }
    }, [taggedFiles, editor, onChange]);

    // Set placeholder based on state
    useEffect(() => {
      if (!editor) return;

      editor.setOptions({
        editorProps: {
          attributes: {
            "data-placeholder": placeholder,
            class: "outline-none",
          },
        },
      });
    }, [editor, placeholder]);

    return (
      <div
        className={cn(
          "min-h-[60px] max-h-[200px] overflow-auto p-3 rounded-md border focus-within:ring-2 focus-within:ring-ring focus-within:ring-primary relative text-foreground",
          isDragging && "border-2 border-dashed border-primary bg-primary/10",
          disabled && "cursor-not-allowed opacity-50",
          "prose prose-sm max-w-none",
          className,
        )}
        data-drag-active={isDragging}
      >
        <EditorContent editor={editor} />
        {!hasContent && !taggedFiles.length && (
          <div className="absolute top-[50%] left-[14px] transform -translate-y-[50%] pointer-events-none text-muted-foreground">
            {placeholder}
          </div>
        )}
      </div>
    );
  },
);

export default TipTapEditor;
