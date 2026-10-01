import './index.css'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'

import App from './App'
import { ApiFailure } from './lib/api'

const rootElement = document.getElementById('root')
if (!rootElement) throw new Error('Failed to find the root element')

const queryClient = new QueryClient({
  defaultOptions: {
    // A 404 or a contract mismatch will not fix itself on a retry; a sleeping
    // host waking up will, once.
    queries: {
      retry: (failures, error) =>
        failures < 1 &&
        !(error instanceof ApiFailure && error.status !== null && error.status < 500),
      refetchOnWindowFocus: true,
    },
  },
})

// BASE_URL is '/' everywhere except GitHub Pages, where the site lives under /<repo>/
// and the router has to know it, or every link would point at the domain root.
createRoot(rootElement).render(
  <QueryClientProvider client={queryClient}>
    <BrowserRouter basename={import.meta.env.BASE_URL}>
      <App />
    </BrowserRouter>
  </QueryClientProvider>,
)
