import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './index.css'

document.addEventListener('contextmenu', e => e.preventDefault());

const container = document.getElementById('root');
if (container) {
  ReactDOM.createRoot(container).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
} else {
  console.error('Cannot mount app: missing #root element');
}
