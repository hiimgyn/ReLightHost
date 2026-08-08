import { Component, type ErrorInfo, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Uncaught render error:', error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{
          display: 'flex', flexDirection: 'column', alignItems: 'center',
          justifyContent: 'center', height: '100vh', gap: '12px', padding: '24px',
          fontFamily: 'sans-serif', textAlign: 'center',
        }}>
          <h2 style={{ margin: 0 }}>Something went wrong</h2>
          <pre style={{
            background: '#1a1a1a', color: '#f87171', padding: '12px', borderRadius: '6px',
            maxWidth: '600px', overflow: 'auto', fontSize: '12px', textAlign: 'left',
          }}>
            {this.state.error.message}
          </pre>
          <button
            onClick={() => window.location.reload()}
            style={{
              padding: '8px 20px', borderRadius: '6px', border: 'none',
              background: '#3b82f6', color: '#fff', cursor: 'pointer', fontSize: '14px',
            }}
          >
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
