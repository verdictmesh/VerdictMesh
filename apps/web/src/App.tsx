import { Route, Routes } from 'react-router-dom'
import Hearing from './pages/Hearing'
import Hearings from './pages/Hearings'
import NotFound from './pages/NotFound'
import PartyView from './pages/PartyView'
import Settlement from './pages/Settlement'

const App = () => (
  <Routes>
    <Route path="/" element={<Hearings />} />
    <Route path="/hearing/:id" element={<Hearing />} />
    <Route path="/hearing/:id/party" element={<PartyView />} />
    <Route path="/settlement/:id" element={<Settlement />} />
    <Route path="*" element={<NotFound />} />
  </Routes>
)

export default App
