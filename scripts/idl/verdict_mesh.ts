/**
 * Program IDL in camelCase format in order to be used in JS/TS.
 *
 * Note that this is only a type helper and is not the actual IDL. The original
 * IDL can be found at `target/idl/verdict_mesh.json`.
 */
export type VerdictMesh = {
  "address": "8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq",
  "metadata": {
    "name": "verdictMesh",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "Protocol-agnostic dispute resolution layer for Solana"
  },
  "docs": [
    "VerdictMesh — a protocol-agnostic dispute resolution layer.",
    "",
    "The program never holds integrator funds: the escrow reads the `Dispute` account as",
    "plain state and distributes the funds itself (see docs/PLAN.md → \"the verdict is",
    "pulled, not pushed\"). That is why there is no instruction here capable of moving",
    "someone else's money — FR-014 is a property of the design, not a promise."
  ],
  "instructions": [
    {
      "name": "attestReport",
      "docs": [
        "Records the fingerprint of the fact-finding report on the dispute.",
        "Signed by the reporter role only, once, before the commit window",
        "closes — see `instructions::attest_report`."
      ],
      "discriminator": [
        244,
        249,
        183,
        49,
        34,
        146,
        190,
        63
      ],
      "accounts": [
        {
          "name": "reporter",
          "docs": [
            "The reporter role named in the protocol config. Not writable: the",
            "transaction fee is paid by whoever pays it, and the role needs no",
            "account of its own."
          ],
          "signer": true,
          "relations": [
            "config"
          ]
        },
        {
          "name": "config",
          "docs": [
            "The protocol config, read for the reporter key it names."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "dispute",
          "docs": [
            "The dispute whose report is attested. Only `report_hash` changes."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "reportHash",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "commitVote",
      "docs": [
        "Records the juror's hidden vote commitment during the commit window. The",
        "vote itself is absent from state until reveal — see `instructions::commit_vote`."
      ],
      "discriminator": [
        134,
        97,
        90,
        126,
        91,
        66,
        16,
        26
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "The juror pays rent for their own vote account. The signature is the only",
            "proof of authorship of the commitment: the signer's wallet goes into both the",
            "account address and the hash itself (`crate::vote`)."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "dispute",
          "docs": [
            "Read-only. `Dispute` deliberately has no counter of submitted commitments:",
            "quorum is counted over **revealed** votes (`FR-010`), and who committed but",
            "stayed silent is visible from the vote accounts, which have to be iterated",
            "for slashing anyway."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "vote",
          "docs": [
            "`init`, not `init_if_needed`: a commitment is submitted once. A second",
            "submission is not \"changing one's mind\" but a chance to wait for someone",
            "else's reveal and rewrite one's own vote to match it.",
            "",
            "The address is derived from the \"dispute + juror\" pair, so a commitment cannot",
            "be written into someone else's account, and each juror has exactly one per dispute."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  111,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "commitment",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "initialize",
      "docs": [
        "One-time protocol initialization: the settlement asset, the reporter role key",
        "and the treasury address. The program has no instruction that changes them",
        "afterwards — see `instructions::initialize`."
      ],
      "discriminator": [
        175,
        175,
        109,
        31,
        13,
        152,
        155,
        237
      ],
      "accounts": [
        {
          "name": "payer",
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "docs": [
            "`init` itself is the guard against re-initialization: an account that already",
            "has an owner cannot be created again."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint",
          "docs": [
            "The protocol's shared settlement asset — `FR-011a`. Its type is checked now,",
            "not on the first stake transfer: a `Config` pointing at a non-mint would break",
            "every later instruction that moves funds, and the trail would lead back here.",
            "",
            "No signature is required: the settlement asset's issuer has nothing to do",
            "with the protocol."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "reporter",
          "type": "pubkey"
        },
        {
          "name": "treasury",
          "type": "pubkey"
        }
      ]
    },
    {
      "name": "openDispute",
      "docs": [
        "Opens a dispute over a locked balance. Called by the escrow program",
        "with its own signature — see `instructions::open_dispute`."
      ],
      "discriminator": [
        137,
        25,
        99,
        119,
        23,
        223,
        161,
        42
      ],
      "accounts": [
        {
          "name": "payer",
          "docs": [
            "Rent for the dispute accounts is paid by whoever initiates the transaction, not by the",
            "escrow PDA: the PDA may hold no lamports, and requiring them from it would mean",
            "requiring the integrator to keep a balance in someone else's program."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint",
          "docs": [
            "The deposit is paid in the protocol's settlement asset, not in the",
            "disputed asset: `FR-011a` keeps juror economics independent of what",
            "exactly the parties are disputing."
          ]
        },
        {
          "name": "depositor",
          "docs": [
            "The deposit is made by **the party opening the dispute** — `FR-026` — and that is exactly",
            "whoever the dispute records as `claimant`. The equality is not a formality: without it",
            "the escrow could open a dispute \"on behalf of\" the claimant yet pay from someone else's",
            "wallet, and `FR-026a` would have nothing to rest on — the review cost would fall on a",
            "person who never knew about the dispute.",
            "",
            "The signature passes through the CPI: the escrow signs the dispute with",
            "its PDA, the claimant signs the outer transaction."
          ],
          "signer": true
        },
        {
          "name": "integrator",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  105,
                  110,
                  116,
                  101,
                  103,
                  114,
                  97,
                  116,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "integrator.authority",
                "account": "integrator"
              }
            ]
          }
        },
        {
          "name": "escrow",
          "docs": [
            "The escrow account whose funds are in dispute. It becomes `escrow_ref` —",
            "the only thing by which the escrow later recognizes \"its\" dispute before",
            "distributing the funds (`FR-012`).",
            "",
            "Hence two requirements, neither redundant. **Owner** — the program the integrator named",
            "at registration: otherwise `escrow_ref` could point to anything. **Signature** — because",
            "ownership is not enough: anyone can read another program's account, and without a",
            "signature it would be enough to pass in someone else's escrow to make it execute a",
            "fabricated verdict.",
            "",
            "program; the owner and the signature are checked."
          ],
          "signer": true
        },
        {
          "name": "dispute",
          "docs": [
            "The dispute number comes from the integrator's counter, not from the",
            "client: otherwise numbering would stop being dense, and `dispute_count`",
            "would no longer tell the truth about how many disputes exist.",
            "",
            "Space is allocated for the **extended** panel — see `Dispute::space`."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "integrator"
              },
              {
                "kind": "account",
                "path": "integrator.dispute_count",
                "account": "integrator"
              }
            ]
          }
        },
        {
          "name": "depositorTokens",
          "writable": true
        },
        {
          "name": "disputeVault",
          "docs": [
            "The vault of this dispute — and only this one. One per dispute, not shared: the appeal",
            "bond (`FR-021`) also lands here and will have to be returned to specific parties, and a",
            "shared vault cannot tell \"whose amount this is\".",
            "",
            "The authority is `Config`, the same PDA as for the stake vault: no",
            "private key exists for it, so the deposit leaves only by a path the",
            "program signs itself (`FR-014`)."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "claimant",
          "type": "pubkey"
        },
        {
          "name": "respondent",
          "type": "pubkey"
        },
        {
          "name": "amount",
          "type": "u64"
        },
        {
          "name": "claimantClaimHash",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        },
        {
          "name": "respondentClaimHash",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "registerIntegrator",
      "docs": [
        "Binds an arbitration policy to the integrator. The policy is validated",
        "once, here, and is copied into every dispute as a snapshot — see",
        "`instructions::register_integrator`."
      ],
      "discriminator": [
        105,
        254,
        83,
        40,
        118,
        70,
        154,
        105
      ],
      "accounts": [
        {
          "name": "authority",
          "writable": true,
          "signer": true
        },
        {
          "name": "integrator",
          "docs": [
            "A PDA keyed by the owner: a protocol can be claimed only with one's own",
            "signature, and someone else's key has nowhere to write its policy."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  105,
                  110,
                  116,
                  101,
                  103,
                  114,
                  97,
                  116,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "authority"
              }
            ]
          }
        },
        {
          "name": "config",
          "docs": [
            "There is no policy without a settlement asset: `juror_stake` and",
            "`deposit` are denominated in it (`FR-011a`). Requiring `Config` makes the",
            "`initialize` → `register_integrator` order a check, not a convention."
          ],
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "escrowProgram",
          "docs": [
            "The integrator's escrow program: `open_dispute` later uses it to recognize that a dispute",
            "is opened by that program's own account. We check that it really is a program — nothing",
            "can update this field, so a mistake in it would be permanent.",
            "",
            "is checked."
          ]
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "policy",
          "type": {
            "defined": {
              "name": "policy"
            }
          }
        }
      ]
    },
    {
      "name": "revealVote",
      "docs": [
        "Reveals the vote and checks it against the submitted commitment. A mismatch",
        "is rejected — see `instructions::reveal_vote`."
      ],
      "discriminator": [
        100,
        157,
        139,
        17,
        186,
        75,
        185,
        149
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "The juror's signature is what ties the vote account to whoever submitted it:",
            "the signer's wallet goes into both the account address and the hash itself.",
            "So no one can reveal another juror's vote with their own signature — neither",
            "on behalf of a juror who chose to stay silent, nor against them.",
            "",
            "`mut` is not needed: revealing creates no accounts and refunds no rent."
          ],
          "signer": true
        },
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "vote",
          "docs": [
            "The account is not closed after reveal: slashing (`FR-011`, `FR-008b`,",
            "T020) reads exactly this account to tell a correct vote from a losing one,",
            "and both from silence. The rent is returned to the juror there."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  118,
                  111,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "choice",
          "type": {
            "defined": {
              "name": "ballot"
            }
          }
        },
        {
          "name": "salt",
          "type": {
            "array": [
              "u8",
              32
            ]
          }
        }
      ]
    },
    {
      "name": "selectPanel",
      "docs": [
        "Selects the juror panel for an open dispute. Permissionless: the outcome",
        "is deterministic and was already fixed when the dispute was opened — see",
        "`instructions::select_panel`."
      ],
      "discriminator": [
        201,
        213,
        164,
        42,
        76,
        78,
        51,
        113
      ],
      "accounts": [
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "registry",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "slotHashes",
          "docs": [
            "Entropy source — `FR-006`. Read as raw bytes: the sysvar's 512 entries",
            "are not deserialized in full (see `panel::entropy_of`).",
            ""
          ],
          "address": "SysvarS1otHashes111111111111111111111111111"
        }
      ],
      "args": []
    },
    {
      "name": "settleStakes",
      "docs": [
        "Slashes losing votes and silence, splits the proceeds among those who were right,",
        "and releases the panel from the registry — see `instructions::settle_stakes`."
      ],
      "discriminator": [
        78,
        170,
        60,
        233,
        48,
        236,
        110,
        215
      ],
      "accounts": [
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        },
        {
          "name": "crank",
          "docs": [
            "Anyone. A signature is required only because the rent of the closed dispute vault has to",
            "go to someone, and the fairest recipient is whoever took it upon themselves to call",
            "the crank."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "disputeVault",
          "docs": [
            "This dispute's vault. It is closed here — hence `mut`."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              },
              {
                "kind": "account",
                "path": "dispute"
              }
            ]
          }
        },
        {
          "name": "stakeVault",
          "docs": [
            "The jurors' share moves here: `Juror.stake` is a record against this vault, and it may",
            "only grow together with it."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  116,
                  97,
                  107,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              }
            ]
          }
        },
        {
          "name": "treasuryTokens",
          "docs": [
            "The protocol treasury's token account — `FR-026b`. Checked by the owner from `Config`",
            "rather than by the account's own address: the ATA address is derived from the owner and",
            "the mint, both already bound, and storing it separately would be a second source of the",
            "same fact."
          ],
          "writable": true
        },
        {
          "name": "tokenProgram"
        }
      ],
      "args": []
    },
    {
      "name": "stake",
      "docs": [
        "Deposits a stake and adds the juror to the registry. There is no entry",
        "threshold: whether the stake is sufficient is decided by the policy of the",
        "dispute whose panel the juror lands on — see `instructions::stake`."
      ],
      "discriminator": [
        206,
        176,
        202,
        18,
        200,
        209,
        179,
        108
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "The juror pays rent for their own accounts and is the transfer authority:",
            "the stake comes from their own wallet, not from someone else's by delegation."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint",
          "docs": [
            "The mint is passed as an account because `transfer_checked` checks decimals",
            "against it. Binding it to `Config` makes substitution impossible: otherwise a",
            "mint with different decimals would turn a hundred units into one hundredth,",
            "and the program would not notice."
          ]
        },
        {
          "name": "registry",
          "docs": [
            "`init_if_needed` is both safe and necessary here: the registry address is",
            "fixed, fields are checked by constraints, and the handler resets nothing — it",
            "only increments the counter, so the \"just created\" and \"already existed\" paths",
            "produce the same result. Creating the registry in `initialize` would mean the",
            "key that initializes the protocol also chooses when the registry appears."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "jurorAccount",
          "docs": [
            "`init`, not `init_if_needed`: one wallet, one registry entry. A second stake",
            "from the same key must fail here rather than overwrite the index."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        },
        {
          "name": "jurorIndex",
          "docs": [
            "The registry slot is derived from the counter, not from an argument: a number",
            "ahead of the counter simply has no address to be created at, so numbering",
            "stays dense. Panel selection (`FR-006`) relies on exactly that."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114,
                  95,
                  105,
                  100,
                  120
                ]
              },
              {
                "kind": "account",
                "path": "registry.juror_count",
                "account": "jurorRegistry"
              }
            ]
          }
        },
        {
          "name": "jurorTokens",
          "writable": true
        },
        {
          "name": "stakeVault",
          "docs": [
            "The shared stake vault. Its authority is `Config`, i.e. a program PDA: by",
            "construction it has no private key, and funds can only leave through what the",
            "program signs itself (`FR-014`). `Config` was chosen because its canonical",
            "bump is already stored in state — every future transfer out of the vault is",
            "signed with a value read from the account rather than re-derived."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  116,
                  97,
                  107,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "tally",
      "docs": [
        "Tallies the vote: a verdict, a one-time escalation, or the status quo.",
        "Permissionless — see `instructions::tally`."
      ],
      "discriminator": [
        152,
        106,
        131,
        171,
        155,
        62,
        41,
        7
      ],
      "accounts": [
        {
          "name": "dispute",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  100,
                  105,
                  115,
                  112,
                  117,
                  116,
                  101
                ]
              },
              {
                "kind": "account",
                "path": "dispute.integrator",
                "account": "dispute"
              },
              {
                "kind": "account",
                "path": "dispute.dispute_id",
                "account": "dispute"
              }
            ]
          }
        }
      ],
      "args": []
    },
    {
      "name": "unstake",
      "docs": [
        "Returns the stake and removes the juror from the registry via swap-remove.",
        "While the juror holds an unfinalized dispute, there is no way out — see",
        "`instructions::unstake`."
      ],
      "discriminator": [
        90,
        95,
        107,
        42,
        205,
        124,
        50,
        225
      ],
      "accounts": [
        {
          "name": "juror",
          "docs": [
            "Only the signer can leave: the juror record is derived from this key, so",
            "there is no way to leave on someone else's behalf."
          ],
          "writable": true,
          "signer": true
        },
        {
          "name": "config",
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  99,
                  111,
                  110,
                  102,
                  105,
                  103
                ]
              }
            ]
          }
        },
        {
          "name": "settlementMint"
        },
        {
          "name": "registry",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  114,
                  101,
                  103,
                  105,
                  115,
                  116,
                  114,
                  121
                ]
              }
            ]
          }
        },
        {
          "name": "jurorAccount",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "juror"
              }
            ]
          }
        },
        {
          "name": "tailIndex",
          "docs": [
            "The registry's last slot. Always closed — the registry shrinks by exactly",
            "the tail each time, which is why no slot beyond the counter can remain that",
            "still points to a juror.",
            "",
            "The rent goes to the juror who is leaving, even though someone else paid",
            "for this slot. Slots are the same size, so each juror pays rent for one",
            "`JurorIndex` and takes back rent for one — there is no difference."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114,
                  95,
                  105,
                  100,
                  120
                ]
              },
              {
                "kind": "account",
                "path": "registry.juror_count.saturating_sub(1)",
                "account": "jurorRegistry"
              }
            ]
          }
        },
        {
          "name": "vacatedIndex",
          "docs": [
            "The slot being vacated. Absent when the last juror leaves — it is then the",
            "same account as `tail_index` and must not be passed as a second input."
          ],
          "writable": true,
          "optional": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114,
                  95,
                  105,
                  100,
                  120
                ]
              },
              {
                "kind": "account",
                "path": "juror_account.index",
                "account": "juror"
              }
            ]
          }
        },
        {
          "name": "mover",
          "docs": [
            "The tail juror — the one who moves into the vacated slot. The address is",
            "derived from the wallet stored in the tail itself, so there is no way to",
            "substitute another juror's record here."
          ],
          "writable": true,
          "optional": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  106,
                  117,
                  114,
                  111,
                  114
                ]
              },
              {
                "kind": "account",
                "path": "tail_index.wallet",
                "account": "jurorIndex"
              }
            ]
          }
        },
        {
          "name": "jurorTokens",
          "writable": true
        },
        {
          "name": "stakeVault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  115,
                  116,
                  97,
                  107,
                  101,
                  95,
                  118,
                  97,
                  117,
                  108,
                  116
                ]
              }
            ]
          }
        },
        {
          "name": "tokenProgram"
        },
        {
          "name": "systemProgram",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": []
    }
  ],
  "accounts": [
    {
      "name": "config",
      "discriminator": [
        155,
        12,
        170,
        224,
        30,
        250,
        204,
        130
      ]
    },
    {
      "name": "dispute",
      "discriminator": [
        36,
        49,
        241,
        67,
        40,
        36,
        241,
        74
      ]
    },
    {
      "name": "integrator",
      "discriminator": [
        188,
        141,
        128,
        161,
        231,
        248,
        0,
        128
      ]
    },
    {
      "name": "juror",
      "discriminator": [
        209,
        201,
        239,
        217,
        237,
        84,
        189,
        152
      ]
    },
    {
      "name": "jurorIndex",
      "discriminator": [
        219,
        120,
        204,
        197,
        63,
        141,
        239,
        3
      ]
    },
    {
      "name": "jurorRegistry",
      "discriminator": [
        155,
        172,
        97,
        99,
        65,
        160,
        139,
        28
      ]
    },
    {
      "name": "voteCommit",
      "discriminator": [
        125,
        216,
        109,
        1,
        40,
        87,
        250,
        47
      ]
    }
  ],
  "events": [
    {
      "name": "depositCollected",
      "discriminator": [
        102,
        8,
        94,
        59,
        116,
        130,
        69,
        8
      ]
    },
    {
      "name": "disputeEscalated",
      "discriminator": [
        118,
        176,
        240,
        103,
        220,
        243,
        199,
        13
      ]
    },
    {
      "name": "disputeFeeSettled",
      "discriminator": [
        217,
        15,
        74,
        25,
        117,
        245,
        126,
        57
      ]
    },
    {
      "name": "disputeFinalized",
      "discriminator": [
        171,
        117,
        216,
        12,
        227,
        254,
        59,
        184
      ]
    },
    {
      "name": "disputeOpened",
      "discriminator": [
        239,
        222,
        102,
        235,
        193,
        85,
        1,
        214
      ]
    },
    {
      "name": "disputeTallied",
      "discriminator": [
        18,
        5,
        81,
        193,
        105,
        254,
        177,
        123
      ]
    },
    {
      "name": "jurorRewarded",
      "discriminator": [
        143,
        67,
        217,
        153,
        227,
        207,
        27,
        159
      ]
    },
    {
      "name": "jurorSlashed",
      "discriminator": [
        195,
        75,
        4,
        101,
        187,
        246,
        186,
        211
      ]
    },
    {
      "name": "jurorStaked",
      "discriminator": [
        208,
        14,
        146,
        223,
        12,
        40,
        63,
        222
      ]
    },
    {
      "name": "jurorUnstaked",
      "discriminator": [
        75,
        4,
        227,
        180,
        94,
        213,
        54,
        23
      ]
    },
    {
      "name": "panelSelected",
      "discriminator": [
        179,
        183,
        76,
        53,
        253,
        179,
        22,
        208
      ]
    },
    {
      "name": "reportAttested",
      "discriminator": [
        211,
        25,
        116,
        220,
        193,
        39,
        39,
        155
      ]
    },
    {
      "name": "voteCommitted",
      "discriminator": [
        74,
        67,
        158,
        48,
        168,
        230,
        217,
        77
      ]
    },
    {
      "name": "voteRevealed",
      "discriminator": [
        104,
        162,
        140,
        194,
        213,
        217,
        117,
        179
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "invalidPolicy",
      "msg": "Policy parameters are inconsistent"
    },
    {
      "code": 6001,
      "name": "registryTooSmall",
      "msg": "Juror registry has fewer jurors than the panel requires"
    },
    {
      "code": 6002,
      "name": "insufficientStake",
      "msg": "Juror stake is below the amount required by the policy"
    },
    {
      "code": 6003,
      "name": "jurorLocked",
      "msg": "Juror still participates in an unfinalized dispute"
    },
    {
      "code": 6004,
      "name": "notOnPanel",
      "msg": "Signer is not on the panel for this dispute"
    },
    {
      "code": 6005,
      "name": "wrongState",
      "msg": "Action is not allowed in the current dispute state"
    },
    {
      "code": 6006,
      "name": "windowClosed",
      "msg": "The window for this action has closed"
    },
    {
      "code": 6007,
      "name": "windowOpen",
      "msg": "The window for this action has not opened yet"
    },
    {
      "code": 6008,
      "name": "commitmentMismatch",
      "msg": "Revealed vote does not match the submitted commitment"
    },
    {
      "code": 6009,
      "name": "notReporter",
      "msg": "Report fingerprint may only be written by the reporter role"
    },
    {
      "code": 6010,
      "name": "reportAlreadyAttested",
      "msg": "Report fingerprint is already set and cannot be changed"
    },
    {
      "code": 6011,
      "name": "alreadyEscalated",
      "msg": "Dispute has already been escalated once"
    },
    {
      "code": 6012,
      "name": "aboveOptimisticThreshold",
      "msg": "Dispute amount is above the optimistic threshold"
    },
    {
      "code": 6013,
      "name": "overflow",
      "msg": "Arithmetic overflow"
    },
    {
      "code": 6014,
      "name": "invalidReporter",
      "msg": "Reporter role cannot be the default key"
    },
    {
      "code": 6015,
      "name": "invalidParties",
      "msg": "A dispute needs two different parties"
    },
    {
      "code": 6016,
      "name": "invalidAmount",
      "msg": "A dispute must be opened over a non-zero locked amount"
    },
    {
      "code": 6017,
      "name": "missingClaim",
      "msg": "Both parties must submit a statement fingerprint"
    },
    {
      "code": 6018,
      "name": "invalidRegistryTail",
      "msg": "Registry tail accounts do not match the slot being vacated"
    },
    {
      "code": 6019,
      "name": "entropyUnavailable",
      "msg": "The entropy slot of this dispute is no longer in SlotHashes"
    },
    {
      "code": 6020,
      "name": "invalidPanelAccounts",
      "msg": "The juror accounts do not enumerate the registry"
    },
    {
      "code": 6021,
      "name": "panelAlreadySelected",
      "msg": "The panel for this dispute has already been selected"
    },
    {
      "code": 6022,
      "name": "alreadyRevealed",
      "msg": "This vote has already been revealed"
    },
    {
      "code": 6023,
      "name": "staleCommitment",
      "msg": "A commitment from an earlier round can no longer be revealed"
    },
    {
      "code": 6024,
      "name": "invalidSettlementAccounts",
      "msg": "The juror accounts do not enumerate the panel of this dispute"
    },
    {
      "code": 6025,
      "name": "invalidTreasury",
      "msg": "Protocol treasury cannot be the default key"
    },
    {
      "code": 6026,
      "name": "notTheDepositor",
      "msg": "The arbitration deposit is paid by the party opening the dispute"
    },
    {
      "code": 6027,
      "name": "emptyReportHash",
      "msg": "Report fingerprint cannot be all zeros"
    }
  ],
  "types": [
    {
      "name": "ballot",
      "docs": [
        "A juror's ballot — **two options, not three**.",
        "",
        "`StatusQuo` is how a failed escalation ends (`FR-027a`), not an answer that can be submitted.",
        "The difference is not a formality: on a panel of three with a quorum of two, three different",
        "answers never form a majority, so a third ballot option would let a single juror send any",
        "dispute to re-review — at someone else's expense, since the protocol pays for escalation",
        "(`FR-027`).",
        "",
        "A separate type rather than a runtime check, because the submission cannot be checked:",
        "`commit_vote` sees only a hash. A juror who committed a `StatusQuo` hash would learn about the",
        "restriction only at reveal — when nothing can be changed any more, and silence costs them a",
        "larger share of their stake (`FR-008b`).",
        "Such a commitment simply cannot be expressed in the type,",
        "so the trap does not exist."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "claimant"
          },
          {
            "name": "respondent"
          }
        ]
      }
    },
    {
      "name": "config",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "settlementMint",
            "type": "pubkey"
          },
          {
            "name": "reporter",
            "docs": [
              "The only privileged key in the system. It can only record a report hash — FR-017a. There",
              "are no instructions through which it can change a verdict or move funds."
            ],
            "type": "pubkey"
          },
          {
            "name": "treasury",
            "docs": [
              "Owner of the token account that receives the protocol's share of the review fee",
              "(`FR-026b`). This is **a destination address, not a role**: no instruction asks it for a",
              "signature or grants it anything beyond receiving a transfer. That is why it does not",
              "conflict with `FR-014` — a key that cannot sign anything has no power over a verdict.",
              "",
              "Written together with the rest of `Config` and likewise never updated: a key able to",
              "change the fee recipient could also change it in the middle",
              "of a review."
            ],
            "type": "pubkey"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "depositCollected",
      "docs": [
        "The review deposit has been paid — `FR-026`. The amount is in the event because it is a",
        "snapshot of the policy at opening time: the integrator may change `Policy` tomorrow, but the",
        "party must see how much it was charged today (`FR-026d`)."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "depositor",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "dispute",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "disputeId",
            "type": "u64"
          },
          {
            "name": "policy",
            "docs": [
              "A snapshot, not a reference — FR-003."
            ],
            "type": {
              "defined": {
                "name": "policy"
              }
            }
          },
          {
            "name": "escrowRef",
            "docs": [
              "PDA of the escrow that opened the dispute. The escrow checks this field against its own",
              "address before executing the verdict — otherwise someone else's dispute could dispose of",
              "its funds."
            ],
            "type": "pubkey"
          },
          {
            "name": "claimant",
            "type": "pubkey"
          },
          {
            "name": "respondent",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "state",
            "type": {
              "defined": {
                "name": "disputeState"
              }
            }
          },
          {
            "name": "panel",
            "docs": [
              "`max_len(0)` is neither a mistake nor \"a panel of zero jurors\". The vector grows up to",
              "`policy.extended_panel_size`, which is known only at opening time, so `InitSpace` counts",
              "just the 4-byte length prefix here and `Dispute::space` adds the rest. This way the macro",
              "still computes the fixed part, and a new field cannot silently drop out of the size",
              "calculation."
            ],
            "type": {
              "vec": "pubkey"
            }
          },
          {
            "name": "reportHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "claimantClaimHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "respondentClaimHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "openedAt",
            "type": "i64"
          },
          {
            "name": "entropySlot",
            "docs": [
              "The slot whose hash provides entropy for panel selection — `FR-006`. Written at opening",
              "and never changed afterwards: if selection took its entropy from the moment of **its own**",
              "execution, it could be gamed by retrying slot after slot until a favorable panel came up.",
              "",
              "This is the slot **before** the one in which the dispute was opened: the current slot's",
              "hash does not exist yet, so selection in the same transaction would",
              "not find it."
            ],
            "type": "u64"
          },
          {
            "name": "commitDeadline",
            "type": "i64"
          },
          {
            "name": "revealDeadline",
            "type": "i64"
          },
          {
            "name": "appealDeadline",
            "type": "i64"
          },
          {
            "name": "votesClaimant",
            "docs": [
              "Revealed votes, one counter per ballot option. There is no separate `revealed_count` for",
              "the same reason `VoteCommit` has no `revealed`: it would always equal the sum of these",
              "two, and two sources of the same value sooner or later diverge. To get how many revealed,",
              "take the sum."
            ],
            "type": "u8"
          },
          {
            "name": "votesRespondent",
            "type": "u8"
          },
          {
            "name": "escalated",
            "docs": [
              "Auto-escalation applies only once — FR-027a."
            ],
            "type": "bool"
          },
          {
            "name": "verdict",
            "type": {
              "option": {
                "defined": {
                  "name": "verdict"
                }
              }
            }
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "disputeEscalated",
      "docs": [
        "The dispute has moved to the extended panel — `FR-027`. The votes that caused the escalation",
        "are in the event itself: otherwise the party sees that the review was extended but does not",
        "see why.",
        "",
        "The list of jurors who did not reveal is deliberately absent, even though `FR-027b` slashes",
        "exactly them. The tally does not know them — it sees two numbers, not vote accounts — and",
        "passing the whole panel's accounts just for an event would hit the transaction limit, while",
        "the observer derives that list on its own anyway: `FR-029` gives it every `VoteCommitted`",
        "without a matching `VoteRevealed`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "votesClaimant",
            "type": "u8"
          },
          {
            "name": "votesRespondent",
            "type": "u8"
          },
          {
            "name": "commitDeadline",
            "type": "i64"
          },
          {
            "name": "revealDeadline",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "disputeFeeSettled",
      "docs": [
        "How the review fee was distributed — `FR-026b`. Two numbers are enough to reconcile the",
        "dispute vault: together they equal what it held, and the account itself is closed",
        "afterwards."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "jurors",
            "type": "u64"
          },
          {
            "name": "protocol",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "disputeFinalized",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "verdict",
            "type": {
              "defined": {
                "name": "verdict"
              }
            }
          },
          {
            "name": "finalizedAt",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "disputeOpened",
      "docs": [
        "FR-029: from events alone, an external observer reconstructs the full timeline of a dispute",
        "without access to the off-chain service. The watcher in apps/api reads exactly these."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "escrowRef",
            "type": "pubkey"
          },
          {
            "name": "claimant",
            "type": "pubkey"
          },
          {
            "name": "respondent",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "optimistic",
            "type": "bool"
          },
          {
            "name": "openedAt",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "disputeState",
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "optimisticPending"
          },
          {
            "name": "committing"
          },
          {
            "name": "revealing"
          },
          {
            "name": "tallied"
          },
          {
            "name": "appealed"
          },
          {
            "name": "finalized"
          }
        ]
      }
    },
    {
      "name": "disputeTallied",
      "docs": [
        "A verdict has been reached — `FR-010`. The counts sit next to the outcome because a party",
        "cannot verify a verdict without its basis."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "verdict",
            "type": {
              "defined": {
                "name": "verdict"
              }
            }
          },
          {
            "name": "votesClaimant",
            "type": "u8"
          },
          {
            "name": "votesRespondent",
            "type": "u8"
          },
          {
            "name": "appealDeadline",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "integrator",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "authority",
            "type": "pubkey"
          },
          {
            "name": "escrowProgram",
            "type": "pubkey"
          },
          {
            "name": "policy",
            "type": {
              "defined": {
                "name": "policy"
              }
            }
          },
          {
            "name": "disputeCount",
            "type": "u64"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "juror",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "wallet",
            "type": "pubkey"
          },
          {
            "name": "stake",
            "docs": [
              "The amount deposited, not a \"sufficient\" amount: sufficiency is determined by the policy",
              "of whichever dispute's panel the juror lands on."
            ],
            "type": "u64"
          },
          {
            "name": "activeDisputes",
            "docs": [
              "How many non-finalized disputes hold this juror. While it is non-zero, the stake cannot be",
              "withdrawn (`FR-007`, T015)."
            ],
            "type": "u16"
          },
          {
            "name": "index",
            "docs": [
              "Position in the registry. Together with `JurorIndex` it makes the registry enumerable for",
              "selection: `index` maps a juror to its slot, `JurorIndex` maps back."
            ],
            "type": "u32"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "jurorIndex",
      "docs": [
        "Makes the registry enumerable by index — without it, deterministic selection (FR-006) cannot",
        "pick N out of M without reading the whole registry off-chain.",
        "A juror leaving is a swap-remove: the last index moves into the freed one."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "wallet",
            "type": "pubkey"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "jurorRegistry",
      "docs": [
        "One registry per protocol, not per integrator: a juror stakes once and becomes eligible for",
        "the panels of every integrator. That is why there is no stake threshold here, and there",
        "cannot be one — `juror_stake` lives in `Policy`, so each integrator has its own. A juror's",
        "eligibility for a specific dispute is checked by panel selection (`FR-006`, T016), not by",
        "joining the registry."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "jurorCount",
            "type": "u32"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "jurorRewarded",
      "docs": [
        "Who received the slashed stake — `FR-011`. Together with `JurorSlashed` this gives the full",
        "settlement balance: the observer sees what was taken and where it went without reading",
        "accounts."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "jurorSlashed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "amount",
            "type": "u64"
          },
          {
            "name": "noReveal",
            "type": "bool"
          }
        ]
      }
    },
    {
      "name": "jurorStaked",
      "docs": [
        "A juror joined the registry. The watcher (T027) builds the list of eligible jurors from",
        "these events without reading every program account: `getProgramAccounts` on every selection",
        "is exactly what `JurorIndex` exists to avoid."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "stake",
            "type": "u64"
          },
          {
            "name": "index",
            "type": "u32"
          },
          {
            "name": "jurorCount",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "jurorUnstaked",
      "docs": [
        "A juror left the registry, with swap-remove. `index` is the freed slot,",
        "`moved` is the juror moved into it from the tail.",
        "The `JurorStaked` / `JurorUnstaked` pair of events is enough to rebuild",
        "the full registry membership — `FR-029`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "stake",
            "type": "u64"
          },
          {
            "name": "index",
            "type": "u32"
          },
          {
            "name": "moved",
            "type": {
              "option": "pubkey"
            }
          },
          {
            "name": "jurorCount",
            "type": "u32"
          }
        ]
      }
    },
    {
      "name": "panelSelected",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "panel",
            "type": {
              "vec": "pubkey"
            }
          },
          {
            "name": "entropySlot",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "policy",
      "docs": [
        "Copied into every dispute when it is opened. An integrator changing its policy does not",
        "affect disputes that are already open — FR-003."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "panelSize",
            "type": "u8"
          },
          {
            "name": "extendedPanelSize",
            "type": "u8"
          },
          {
            "name": "quorum",
            "type": "u8"
          },
          {
            "name": "extendedQuorum",
            "type": "u8"
          },
          {
            "name": "jurorStake",
            "type": "u64"
          },
          {
            "name": "slashBpsWrong",
            "type": "u16"
          },
          {
            "name": "slashBpsNoReveal",
            "type": "u16"
          },
          {
            "name": "commitWindow",
            "type": "i64"
          },
          {
            "name": "revealWindow",
            "type": "i64"
          },
          {
            "name": "appealWindow",
            "type": "i64"
          },
          {
            "name": "optimisticWindow",
            "type": "i64"
          },
          {
            "name": "deposit",
            "type": "u64"
          },
          {
            "name": "optimisticThreshold",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "reportAttested",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "reportHash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          }
        ]
      }
    },
    {
      "name": "verdict",
      "docs": [
        "StatusQuo means \"as if there had been no dispute\" (FR-027a). The escrow must be able to",
        "distribute funds under this outcome, otherwise auto-escalation leads nowhere."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "claimant"
          },
          {
            "name": "respondent"
          },
          {
            "name": "statusQuo"
          }
        ]
      }
    },
    {
      "name": "voteCommit",
      "docs": [
        "A juror's vote commitment — `FR-008`. Created during the commit window, and until reveal it",
        "contains nothing from which the vote itself can be derived: `commitment` is a hash",
        "(`crate::vote`), `choice` is `None`.",
        "",
        "**There is deliberately no `revealed` flag here**, even though the data model in `PLAN.md`",
        "lists one. It would always duplicate `choice.is_some()`, and two fields that must agree",
        "sooner or later diverge: stake settlement (T020) distinguishes an unrevealed vote from a",
        "revealed one and must do so from a single source.",
        "That source is `choice`."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "commitment",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "choice",
            "docs": [
              "`None` until the vote is revealed (T018). An empty value is exactly what `FR-008b` means",
              "by \"the juror committed but did not reveal\"."
            ],
            "type": {
              "option": {
                "defined": {
                  "name": "ballot"
                }
              }
            }
          },
          {
            "name": "round",
            "docs": [
              "The review round in which the commitment was submitted — `Dispute::round`. It can only be",
              "revealed in the same round: otherwise a juror who stayed silent in the first round would",
              "decide in the second whether to disclose their vote, already knowing how the first tally",
              "ended. `FR-027b` slashes them precisely for that silence, and a late reveal must not undo",
              "it."
            ],
            "type": "u8"
          },
          {
            "name": "bump",
            "type": "u8"
          }
        ]
      }
    },
    {
      "name": "voteCommitted",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          }
        ]
      }
    },
    {
      "name": "voteRevealed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "juror",
            "type": "pubkey"
          },
          {
            "name": "choice",
            "type": {
              "defined": {
                "name": "ballot"
              }
            }
          }
        ]
      }
    }
  ]
};
