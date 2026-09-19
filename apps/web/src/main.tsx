import './index.css'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'

import App from './App'

const rootElement = document.getElementById('root')
if (!rootElement) throw new Error('Failed to find the root element')

// BASE_URL is '/' everywhere except GitHub Pages, where the site lives under /<repo>/
// and the router has to know it, or every link would point at the domain root.
createRoot(rootElement).render(
  <BrowserRouter basename={import.meta.env.BASE_URL}>
    <App />
  </BrowserRouter>,
)
