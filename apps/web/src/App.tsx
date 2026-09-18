import { Route, Routes } from 'react-router-dom'
import HearingJuror from './pages/HearingJuror'
import HearingParty from './pages/HearingParty'
import JurorPanel from './pages/JurorPanel'
import NotFound from './pages/NotFound'
import Settlement from './pages/Settlement'

const App = () => (
  <Routes>
    <Route path="/" element={<JurorPanel />} />
    <Route path="/hearing/:id" element={<HearingJuror />} />
    <Route path="/hearing/:id/party" element={<HearingParty />} />
    <Route path="/settlement/:id" element={<Settlement />} />
    <Route path="*" element={<NotFound />} />
  </Routes>
)

export default App
