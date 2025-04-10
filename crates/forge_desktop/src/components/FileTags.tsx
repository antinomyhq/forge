import React from "react";
import FileTag from "./FileTag";

interface FileTagsProps {
  files: string[];
  onRemoveFile: (index: number) => void;
}

const FileTags: React.FC<FileTagsProps> = ({ files, onRemoveFile }) => {
  if (files.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-1 pt-0.5 px-1">
      {files.map((filePath, index) => (
        <FileTag
          key={`${filePath}-${index}`}
          filePath={filePath}
          onRemove={() => onRemoveFile(index)}
        />
      ))}
    </div>
  );
};

export default FileTags;
