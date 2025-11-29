import { Effect } from "effect";
import { StreamDelta } from "../models/StreamDelta";

/// StreamingService provides business logic for handling streaming data
export class StreamingService {
  /// Merges multiple stream deltas into a single string
  static mergeDeltas(deltas: ReadonlyArray<StreamDelta>): Effect.Effect<string> {
    return Effect.succeed(deltas.map((d) => d.content).join(""));
  }
}
