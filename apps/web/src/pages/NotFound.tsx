import { Link } from 'react-router-dom'

const NotFound = () => (
  <div className="flex min-h-screen items-center justify-center bg-background">
    <div className="text-center">
      <p className="mono text-[12px] tracking-[0.16em] text-unestablished">NO SUCH SCREEN</p>
      <Link to="/" className="mt-3 inline-block text-[13.5px] text-foreground underline">
        Return to the juror panel
      </Link>
    </div>
  </div>
)

export default NotFound
