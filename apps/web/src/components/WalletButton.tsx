import { Wallet as WalletIcon } from 'lucide-react'
import { useState } from 'react'
import { shortenMiddle } from '@/lib/dispute'
import { useWallet } from '@/lib/wallet'

/**
 * Connect a wallet to vote. Optional on every screen: watching a hearing needs
 * none (`FR-028a`).
 */
const WalletButton = () => {
  const wallet = useWallet()
  const [open, setOpen] = useState(false)

  if (wallet.address) {
    return (
      <button
        type="button"
        onClick={() => void wallet.disconnect()}
        title="Disconnect"
        className="focus-ring label-xs mono flex items-center gap-1.5 rounded-sm border border-border px-2 py-1.5 text-foreground transition-colors hover:border-border-strong"
      >
        <WalletIcon className="h-3 w-3 text-confirmed" />
        {shortenMiddle(wallet.address, 4, 4)}
      </button>
    )
  }

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="focus-ring label-xs flex items-center gap-1.5 rounded-sm border border-border px-2 py-1.5 text-muted-foreground transition-colors hover:border-border-strong hover:text-foreground"
      >
        <WalletIcon className="h-3 w-3" />
        {wallet.connecting ? 'Connecting…' : 'Juror wallet'}
      </button>
      {open ? (
        <div className="absolute right-0 top-full z-30 mt-1.5 w-64 rounded border border-border-strong bg-surface p-2 shadow-lg">
          {wallet.wallets.length === 0 ? (
            <p className="px-2 py-1.5 text-[12.5px] leading-relaxed text-muted-foreground">
              No Solana wallet found in this browser. Install Phantom, Solflare or Backpack and
              switch it to devnet.
            </p>
          ) : (
            <ul className="flex flex-col">
              {wallet.wallets.map((candidate) => (
                <li key={candidate.name}>
                  <button
                    type="button"
                    onClick={() => {
                      setOpen(false)
                      void wallet.connect(candidate)
                    }}
                    className="focus-ring flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] text-foreground hover:bg-surface-2"
                  >
                    {candidate.icon ? (
                      <img src={candidate.icon} alt="" className="h-4 w-4" />
                    ) : null}
                    {candidate.name}
                  </button>
                </li>
              ))}
            </ul>
          )}
          <p className="mt-1.5 border-t border-border px-2 pt-1.5 text-[11.5px] leading-snug text-unestablished">
            Only panel members need one. Nothing about it is sent to our service.
          </p>
        </div>
      ) : null}
    </div>
  )
}

export default WalletButton
