import { Schema as S } from "effect";

/// FileContext represents a file that can be attached to messages
export class FileContext extends S.Class<FileContext>("FileContext")({
  filePath: S.String,
  content: S.String,
  language: S.String,
  isTagged: S.Boolean,
}) {}
