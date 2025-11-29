import { Context, Effect, Layer, Stream, Ref, Duration, Option, Queue } from "effect";
import { AppState } from "@shared/types/state";
import { StateAction } from "@shared/types/actions";

/// StateService provides centralized state management
export interface StateService {
  readonly state: Ref.Ref<AppState>;
  readonly subscribe: <A>(selector: (state: AppState) => A) => Stream.Stream<A>;
  readonly dispatch: (action: StateAction) => Effect.Effect<void>;
}

export const StateService = Context.GenericTag<StateService>("StateService");

/// Reducer function for state mutations
export const reducer = (state: AppState, action: StateAction): AppState => {
  switch (action._tag) {
    case "ConversationAdded":
      return {
        ...state,
        conversations: [...state.conversations, action.conversation],
      };

    case "ConversationRemoved":
      return {
        ...state,
        conversations: state.conversations.filter((conv) => conv.id.value !== action.conversationId),
      };

    case "MessageAdded":
      return {
        ...state,
        conversations: state.conversations.map((conv) =>
          conv.id.value === action.conversationId
            ? { ...conv, messages: [...conv.messages, action.message] }
            : conv
        ),
      };

    case "ActiveConversationChanged":
      return {
        ...state,
        activeConversationId: Option.some(action.id),
      };

    case "StreamDeltaReceived":
      return {
        ...state,
        streamingState: {
          isStreaming: true,
          currentDelta: state.streamingState.currentDelta + action.delta,
        },
      };

    case "StreamEnded":
      return {
        ...state,
        streamingState: { isStreaming: false, currentDelta: "" },
      };

    case "SidebarToggled":
      return {
        ...state,
        ui: { ...state.ui, sidebarOpen: !state.ui.sidebarOpen },
      };

    case "SettingsToggled":
      return {
        ...state,
        ui: { ...state.ui, settingsOpen: !state.ui.settingsOpen },
      };

    case "ThemeChanged":
      return {
        ...state,
        ui: { ...state.ui, theme: action.theme },
      };

    default:
      return state;
  }
};

/// StateServiceLive implements the state service
export const StateServiceLive = Layer.scoped(
  StateService,
  Effect.gen(function* () {
    const stateRef = yield* Ref.make<AppState>({
      conversations: [],
      activeConversationId: Option.none(),
      streamingState: { isStreaming: false, currentDelta: "" },
      ui: { sidebarOpen: true, settingsOpen: false, theme: "dark" },
    });

    const queue = yield* Queue.unbounded<AppState>();

    // Publish state changes to queue
    yield* Effect.forever(
      Ref.get(stateRef).pipe(
        Effect.flatMap((state) => Queue.offer(queue, state)),
        Effect.delay(Duration.millis(16)) // 60fps
      )
    ).pipe(Effect.forkScoped);

    return StateService.of({
      state: stateRef,

      subscribe: <A>(selector: (state: AppState) => A) =>
        Stream.fromQueue(queue).pipe(Stream.map(selector), Stream.changes),

      dispatch: (action: StateAction) => Ref.update(stateRef, (state) => reducer(state, action)),
    });
  })
);
