import { Component, ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

/**
 * React Error Boundary that handles Effect errors gracefully
 * Integrates with Effect's error types (ValidationError, RpcError, etc.)
 */
export class EffectErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { error, errorInfo: null };
  }

  override componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('[EffectErrorBoundary] Error caught:', error);
    console.error('[EffectErrorBoundary] Error info:', errorInfo);
    
    this.setState({
      error,
      errorInfo,
    });
  }

  reset = () => {
    this.setState({ error: null, errorInfo: null });
  };

  override render() {
    if (this.state.error) {
      if (this.props.fallback) {
        return this.props.fallback(this.state.error, this.reset);
      }

      return <DefaultErrorFallback error={this.state.error} reset={this.reset} />;
    }

    return this.props.children;
  }
}

/**
 * Default error fallback UI
 */
function DefaultErrorFallback({ error, reset }: { error: Error; reset: () => void }) {
  // Check if it's an Effect error with _tag
  const effectError = error as any;
  const isEffectError = effectError._tag !== undefined;

  const getErrorMessage = () => {
    if (isEffectError) {
      switch (effectError._tag) {
        case 'ValidationError':
          return `Validation Error: ${effectError.reason || 'Invalid data'}`;
        case 'RpcError':
          return `RPC Error: ${effectError.message || 'Communication failed'} (Method: ${effectError.method || 'unknown'})`;
        case 'RpcTimeoutError':
          return `Request Timeout: ${effectError.method || 'Request'} took too long`;
        case 'ApplicationError':
          return `Application Error: ${effectError.message || 'Something went wrong'}`;
        default:
          return effectError.message || 'An unexpected error occurred';
      }
    }
    return error.message || 'An unexpected error occurred';
  };

  const getErrorDetails = () => {
    if (isEffectError && effectError.cause) {
      return JSON.stringify(effectError.cause, null, 2);
    }
    return error.stack;
  };

  return (
    <div 
      style={{
        padding: '20px',
        margin: '20px',
        border: '2px solid var(--vscode-errorForeground)',
        borderRadius: '4px',
        backgroundColor: 'var(--vscode-inputValidation-errorBackground)',
        color: 'var(--vscode-errorForeground)',
      }}
    >
      <h2 style={{ margin: '0 0 10px 0' }}>❌ Error</h2>
      <p style={{ margin: '0 0 10px 0', fontWeight: 'bold' }}>{getErrorMessage()}</p>
      
      {isEffectError && (
        <p style={{ margin: '0 0 10px 0', fontSize: '0.9em', opacity: 0.8 }}>
          Error Type: {effectError._tag}
        </p>
      )}

      <details style={{ marginTop: '10px' }}>
        <summary style={{ cursor: 'pointer', userSelect: 'none' }}>
          Show Details
        </summary>
        <pre style={{
          marginTop: '10px',
          padding: '10px',
          backgroundColor: 'var(--vscode-editor-background)',
          border: '1px solid var(--vscode-panel-border)',
          borderRadius: '2px',
          overflow: 'auto',
          fontSize: '0.85em',
        }}>
          {getErrorDetails()}
        </pre>
      </details>

      <button
        onClick={reset}
        style={{
          marginTop: '15px',
          padding: '8px 16px',
          backgroundColor: 'var(--vscode-button-background)',
          color: 'var(--vscode-button-foreground)',
          border: 'none',
          borderRadius: '2px',
          cursor: 'pointer',
          fontFamily: 'inherit',
        }}
        onMouseOver={(e) => {
          e.currentTarget.style.backgroundColor = 'var(--vscode-button-hoverBackground)';
        }}
        onMouseOut={(e) => {
          e.currentTarget.style.backgroundColor = 'var(--vscode-button-background)';
        }}
      >
        Try Again
      </button>
    </div>
  );
}

/**
 * Hook-based error boundary helper
 * Use this to wrap Effect operations with error handling
 */
export function withEffectErrorHandling<A, E>(
  effect: () => Promise<A>,
  onError?: (error: E) => void
): () => Promise<A> {
  return async () => {
    try {
      return await effect();
    } catch (error) {
      console.error('[withEffectErrorHandling] Error:', error);
      if (onError) {
        onError(error as E);
      }
      throw error; // Re-throw to let Error Boundary catch it
    }
  };
}
