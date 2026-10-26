/**
 * The IDL of the `depin_escrow` program as a **value**: `BorshCoder` decodes
 * both accounts and events with it, and outside `target/` there is nowhere
 * else to get it.
 *
 * Generated from `target/idl/depin_escrow.json` by `sync-idl` in `scripts/`
 * — editing it by hand is pointless, the next build overwrites it anyway. Field
 * names here are snake_case, the way `anchor build` leaves them; `Program`
 * camel-cases them, `BorshCoder` does not, and this is the form it has to be
 * fed.
 */

import type { Idl } from '@coral-xyz/anchor'

export const depinEscrowIdl = {
  "address": "DaQXW5oL5bDH77XxJ4MoSvwTv6X7GwqNixkNYGNLrFk8",
  "metadata": {
    "name": "depin_escrow",
    "version": "0.1.0",
    "spec": "0.1.0",
    "description": "DePIN reward escrow that splits an epoch between network and operator and resolves disputes through VerdictMesh"
  },
  "docs": [
    "A DePIN reward escrow that buys arbitration from VerdictMesh — the second",
    "integration, built to be unlike the first (`reference_escrow`).",
    "",
    "A network locks an epoch's reward for one of its operators, then reports",
    "what share of it the operator earned. The operator either lets the report",
    "stand or challenges it with a counter-offer, and the panel picks one of the",
    "two offers. Whichever wins, the reward is **split** between both parties;",
    "the loser's share is where the review deposit is reimbursed from, so there",
    "is no bond vault. No milestones, asymmetric roles, positions that are",
    "numbers rather than roles — and the same `open_dispute` CPI and",
    "`Dispute::ruling` read as the milestone escrow uses."
  ],
  "instructions": [
    {
      "name": "dispute_epoch",
      "docs": [
        "The operator challenges the report with a counter-offer — one CPI into",
        "VerdictMesh. See `instructions::dispute`."
      ],
      "discriminator": [
        86,
        215,
        188,
        85,
        141,
        89,
        211,
        209
      ],
      "accounts": [
        {
          "name": "operator",
          "docs": [
            "Pays the review deposit (`FR-026`) and the dispute's rent."
          ],
          "writable": true,
          "signer": true,
          "relations": [
            "epoch"
          ]
        },
        {
          "name": "epoch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "epoch.network",
                "account": "Epoch"
              },
              {
                "kind": "account",
                "path": "epoch.epoch_id",
                "account": "Epoch"
              }
            ]
          }
        },
        {
          "name": "integrator",
          "docs": [
            "policy the epoch was funded under."
          ],
          "writable": true
        },
        {
          "name": "config"
        },
        {
          "name": "settlement_mint"
        },
        {
          "name": "dispute",
          "writable": true
        },
        {
          "name": "claimant_tokens",
          "writable": true
        },
        {
          "name": "dispute_vault",
          "writable": true
        },
        {
          "name": "verdict_mesh_program",
          "address": "8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq"
        },
        {
          "name": "token_program"
        },
        {
          "name": "system_program",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "claimed_bps",
          "type": "u16"
        }
      ]
    },
    {
      "name": "fund_epoch",
      "docs": [
        "Locks the reward. Both parties sign: with the epoch the operator accepts",
        "the review policy. See `instructions::epoch`."
      ],
      "discriminator": [
        62,
        200,
        22,
        58,
        233,
        83,
        165,
        196
      ],
      "accounts": [
        {
          "name": "network",
          "writable": true,
          "signer": true
        },
        {
          "name": "operator",
          "docs": [
            "Signs with the network: they accept the policy they will be judged by."
          ],
          "signer": true
        },
        {
          "name": "integrator",
          "docs": [
            "Typed with VerdictMesh's type, so Anchor checks the owner as well — a",
            "made-up `Integrator` does not pass."
          ]
        },
        {
          "name": "config",
          "docs": [
            "Needed only for `settlement_mint`. Created once at a fixed address, so",
            "no other account of this type owned by VerdictMesh exists."
          ]
        },
        {
          "name": "mint"
        },
        {
          "name": "epoch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "network"
              },
              {
                "kind": "arg",
                "path": "epoch_id"
              }
            ]
          }
        },
        {
          "name": "network_tokens",
          "writable": true
        },
        {
          "name": "vault",
          "docs": [
            "The reward vault, owned by the epoch's PDA."
          ],
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104,
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
                "path": "epoch"
              }
            ]
          }
        },
        {
          "name": "token_program"
        },
        {
          "name": "system_program",
          "address": "11111111111111111111111111111111"
        }
      ],
      "args": [
        {
          "name": "epoch_id",
          "type": "u64"
        },
        {
          "name": "reward",
          "type": "u64"
        },
        {
          "name": "challenge_window",
          "type": "i64"
        }
      ]
    },
    {
      "name": "report_usage",
      "docs": [
        "The network publishes the operator's share; the challenge window",
        "starts. See `instructions::epoch`."
      ],
      "discriminator": [
        65,
        82,
        222,
        97,
        7,
        67,
        132,
        82
      ],
      "accounts": [
        {
          "name": "network",
          "signer": true,
          "relations": [
            "epoch"
          ]
        },
        {
          "name": "epoch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "epoch.network",
                "account": "Epoch"
              },
              {
                "kind": "account",
                "path": "epoch.epoch_id",
                "account": "Epoch"
              }
            ]
          }
        }
      ],
      "args": [
        {
          "name": "operator_bps",
          "type": "u16"
        }
      ]
    },
    {
      "name": "settle_dispute",
      "docs": [
        "Executes the verdict: pays out the winning offer and reimburses the",
        "deposit from the loser's share. Anyone may send it. See",
        "`instructions::settle`."
      ],
      "discriminator": [
        155,
        147,
        5,
        44,
        20,
        204,
        146,
        43
      ],
      "accounts": [
        {
          "name": "epoch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "epoch.network",
                "account": "Epoch"
              },
              {
                "kind": "account",
                "path": "epoch.epoch_id",
                "account": "Epoch"
              }
            ]
          }
        },
        {
          "name": "dispute",
          "docs": [
            "Typed with VerdictMesh's type, so Anchor checks the owner: a `Dispute`",
            "laid out by another program cannot write itself a verdict. Read-only —",
            "the escrow changes nothing in someone else's state."
          ]
        },
        {
          "name": "mint"
        },
        {
          "name": "network_tokens",
          "writable": true
        },
        {
          "name": "operator_tokens",
          "writable": true
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104,
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
                "path": "epoch"
              }
            ]
          }
        },
        {
          "name": "token_program"
        }
      ],
      "args": []
    },
    {
      "name": "settle_report",
      "docs": [
        "Pays out an unchallenged report once its window has closed. Anyone may",
        "send it. See `instructions::epoch`."
      ],
      "discriminator": [
        175,
        186,
        68,
        94,
        87,
        45,
        127,
        107
      ],
      "accounts": [
        {
          "name": "epoch",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104
                ]
              },
              {
                "kind": "account",
                "path": "epoch.network",
                "account": "Epoch"
              },
              {
                "kind": "account",
                "path": "epoch.epoch_id",
                "account": "Epoch"
              }
            ]
          }
        },
        {
          "name": "mint"
        },
        {
          "name": "network_tokens",
          "writable": true
        },
        {
          "name": "operator_tokens",
          "writable": true
        },
        {
          "name": "vault",
          "writable": true,
          "pda": {
            "seeds": [
              {
                "kind": "const",
                "value": [
                  101,
                  112,
                  111,
                  99,
                  104,
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
                "path": "epoch"
              }
            ]
          }
        },
        {
          "name": "token_program"
        }
      ],
      "args": []
    }
  ],
  "accounts": [
    {
      "name": "Config",
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
      "name": "Dispute",
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
      "name": "Epoch",
      "discriminator": [
        93,
        83,
        120,
        89,
        151,
        138,
        152,
        108
      ]
    },
    {
      "name": "Integrator",
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
    }
  ],
  "events": [
    {
      "name": "EpochDisputed",
      "discriminator": [
        71,
        237,
        102,
        107,
        206,
        243,
        185,
        75
      ]
    },
    {
      "name": "EpochFunded",
      "discriminator": [
        195,
        100,
        72,
        55,
        149,
        43,
        110,
        236
      ]
    },
    {
      "name": "EpochSettled",
      "discriminator": [
        32,
        219,
        45,
        156,
        250,
        115,
        190,
        255
      ]
    },
    {
      "name": "UsageReported",
      "discriminator": [
        126,
        184,
        245,
        114,
        242,
        54,
        14,
        99
      ]
    }
  ],
  "errors": [
    {
      "code": 6000,
      "name": "InvalidParties",
      "msg": "An epoch needs two different parties"
    },
    {
      "code": 6001,
      "name": "InvalidReward",
      "msg": "The epoch must lock a reward"
    },
    {
      "code": 6002,
      "name": "InvalidWindow",
      "msg": "The challenge window must be positive"
    },
    {
      "code": 6003,
      "name": "WrongArbitrationProgram",
      "msg": "The integrator record points at another escrow program"
    },
    {
      "code": 6004,
      "name": "WrongRewardMint",
      "msg": "The reward must be held in the settlement asset of the protocol"
    },
    {
      "code": 6005,
      "name": "InvalidShare",
      "msg": "A share is at most 10000 basis points"
    },
    {
      "code": 6006,
      "name": "NotTheNetwork",
      "msg": "Only the network of this epoch may do this"
    },
    {
      "code": 6007,
      "name": "NotTheOperator",
      "msg": "Only the operator of this epoch may do this"
    },
    {
      "code": 6008,
      "name": "AlreadyReported",
      "msg": "Usage for this epoch has already been reported"
    },
    {
      "code": 6009,
      "name": "NotReported",
      "msg": "The epoch has no open report"
    },
    {
      "code": 6010,
      "name": "ChallengeWindowClosed",
      "msg": "The challenge window has closed"
    },
    {
      "code": 6011,
      "name": "ChallengeWindowOpen",
      "msg": "The challenge window is still open"
    },
    {
      "code": 6012,
      "name": "ClaimNotAboveReport",
      "msg": "A challenge must claim more than the report gives"
    },
    {
      "code": 6013,
      "name": "NothingContested",
      "msg": "The two offers differ by less than one token unit"
    },
    {
      "code": 6014,
      "name": "WrongIntegrator",
      "msg": "The dispute must be opened under the policy the epoch was funded with"
    },
    {
      "code": 6015,
      "name": "NotUnderThisDispute",
      "msg": "This epoch is not under the dispute that was brought"
    },
    {
      "code": 6016,
      "name": "VerdictNamesAStranger",
      "msg": "The verdict names a party that is not in this epoch"
    },
    {
      "code": 6017,
      "name": "Overflow",
      "msg": "Arithmetic overflow"
    }
  ],
  "types": [
    {
      "name": "Config",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "settlement_mint",
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
      "name": "Dispute",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "dispute_id",
            "type": "u64"
          },
          {
            "name": "policy",
            "docs": [
              "A snapshot, not a reference — FR-003."
            ],
            "type": {
              "defined": {
                "name": "Policy"
              }
            }
          },
          {
            "name": "escrow_ref",
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
                "name": "DisputeState"
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
            "name": "report_hash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "claimant_claim_hash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "respondent_claim_hash",
            "type": {
              "array": [
                "u8",
                32
              ]
            }
          },
          {
            "name": "opened_at",
            "type": "i64"
          },
          {
            "name": "entropy_slot",
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
            "name": "commit_deadline",
            "type": "i64"
          },
          {
            "name": "reveal_deadline",
            "type": "i64"
          },
          {
            "name": "appeal_deadline",
            "type": "i64"
          },
          {
            "name": "votes_claimant",
            "docs": [
              "Revealed votes, one counter per ballot option. There is no separate `revealed_count` for",
              "the same reason `VoteCommit` has no `revealed`: it would always equal the sum of these",
              "two, and two sources of the same value sooner or later diverge. To get how many revealed,",
              "take the sum."
            ],
            "type": "u8"
          },
          {
            "name": "votes_respondent",
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
                  "name": "Verdict"
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
      "name": "DisputeState",
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "OptimisticPending"
          },
          {
            "name": "Committing"
          },
          {
            "name": "Revealing"
          },
          {
            "name": "Tallied"
          },
          {
            "name": "Appealed"
          },
          {
            "name": "Finalized"
          }
        ]
      }
    },
    {
      "name": "Epoch",
      "docs": [
        "One reward epoch between a DePIN network and one of its operators.",
        "",
        "The network locks the reward up front, serves the epoch, then reports how",
        "much of it the operator earned. Unlike a milestone deal the payout is never",
        "all-or-nothing: whichever split wins, **both** parties are paid a share —",
        "and that share of the loser is what reimburses the review deposit",
        "(`FR-026a`), so this escrow needs no bond vault (`FR-026e`)."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "network",
            "docs": [
              "Locks the reward and reports usage."
            ],
            "type": "pubkey"
          },
          {
            "name": "operator",
            "docs": [
              "Earns a share of the reward and may challenge the report."
            ],
            "type": "pubkey"
          },
          {
            "name": "mint",
            "docs": [
              "The reward asset. Always the protocol's settlement asset: the deposit is",
              "reimbursed out of the loser's share, which only works in the asset the",
              "deposit was paid in (`FR-011a`)."
            ],
            "type": "pubkey"
          },
          {
            "name": "integrator",
            "docs": [
              "PDA `Integrator` in VerdictMesh — the policy both parties signed up to."
            ],
            "type": "pubkey"
          },
          {
            "name": "reward",
            "type": "u64"
          },
          {
            "name": "challenge_window",
            "docs": [
              "How long the operator has to challenge a report, in seconds. Fixed when",
              "the epoch is funded, so the network cannot shorten it after the fact."
            ],
            "type": "i64"
          },
          {
            "name": "epoch_id",
            "docs": [
              "The network's own number for the epoch; part of the address."
            ],
            "type": "u64"
          },
          {
            "name": "state",
            "type": {
              "defined": {
                "name": "EpochState"
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
      "name": "EpochDisputed",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "epoch",
            "type": "pubkey"
          },
          {
            "name": "dispute",
            "type": "pubkey"
          },
          {
            "name": "reported_bps",
            "type": "u16"
          },
          {
            "name": "claimed_bps",
            "type": "u16"
          },
          {
            "name": "contested",
            "docs": [
              "The part of the reward the two offers disagree on — the dispute's",
              "`amount`."
            ],
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "EpochFunded",
      "docs": [
        "Events are what fact-finding rebuilds the **subject** of a dispute from:",
        "what was locked, what the network reported and when, what the operator",
        "claimed (`FR-015`, `FR-016`)."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "epoch",
            "type": "pubkey"
          },
          {
            "name": "network",
            "type": "pubkey"
          },
          {
            "name": "operator",
            "type": "pubkey"
          },
          {
            "name": "integrator",
            "type": "pubkey"
          },
          {
            "name": "reward",
            "type": "u64"
          },
          {
            "name": "challenge_window",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "EpochSettled",
      "docs": [
        "The reward is paid out. `dispute` is empty when the report stood",
        "unchallenged. The verdict itself is not repeated: VerdictMesh announced it",
        "(`DisputeTallied`), and a second record of the same fact would sooner or",
        "later disagree with the first."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "epoch",
            "type": "pubkey"
          },
          {
            "name": "dispute",
            "type": {
              "option": "pubkey"
            }
          },
          {
            "name": "operator_bps",
            "type": "u16"
          },
          {
            "name": "operator_amount",
            "type": "u64"
          },
          {
            "name": "network_amount",
            "type": "u64"
          },
          {
            "name": "reimbursed",
            "docs": [
              "How much of the network's share reimbursed the operator's review",
              "deposit — `FR-026a`. Zero when the network won, when the escalation",
              "failed, or when there was no dispute."
            ],
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "EpochState",
      "docs": [
        "Where the epoch stands.",
        "",
        "**`Disputed` carries both offers and the address of its review**, not a",
        "flag. The offers are what the verdict chooses between — final-offer",
        "arbitration, so the escrow must remember exactly what each side put on the",
        "table. The address is what ties the payout to this review and no other: a",
        "second dispute over the same epoch cannot exist, but a forged one laid out",
        "with the same `escrow_ref` would match on that field alone (`FR-013`)."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "Funded"
          },
          {
            "name": "Reported",
            "fields": [
              {
                "name": "operator_bps",
                "type": "u16"
              },
              {
                "name": "challenge_deadline",
                "type": "i64"
              }
            ]
          },
          {
            "name": "Disputed",
            "fields": [
              {
                "name": "dispute",
                "type": "pubkey"
              },
              {
                "name": "reported_bps",
                "type": "u16"
              },
              {
                "name": "claimed_bps",
                "type": "u16"
              }
            ]
          },
          {
            "name": "Settled",
            "fields": [
              {
                "name": "operator_bps",
                "type": "u16"
              }
            ]
          }
        ]
      }
    },
    {
      "name": "Integrator",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "authority",
            "type": "pubkey"
          },
          {
            "name": "escrow_program",
            "type": "pubkey"
          },
          {
            "name": "policy",
            "type": {
              "defined": {
                "name": "Policy"
              }
            }
          },
          {
            "name": "dispute_count",
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
      "name": "Policy",
      "docs": [
        "Copied into every dispute when it is opened. An integrator changing its policy does not",
        "affect disputes that are already open — FR-003."
      ],
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "panel_size",
            "type": "u8"
          },
          {
            "name": "extended_panel_size",
            "type": "u8"
          },
          {
            "name": "quorum",
            "type": "u8"
          },
          {
            "name": "extended_quorum",
            "type": "u8"
          },
          {
            "name": "juror_stake",
            "type": "u64"
          },
          {
            "name": "slash_bps_wrong",
            "type": "u16"
          },
          {
            "name": "slash_bps_no_reveal",
            "type": "u16"
          },
          {
            "name": "commit_window",
            "type": "i64"
          },
          {
            "name": "reveal_window",
            "type": "i64"
          },
          {
            "name": "appeal_window",
            "type": "i64"
          },
          {
            "name": "optimistic_window",
            "type": "i64"
          },
          {
            "name": "deposit",
            "type": "u64"
          },
          {
            "name": "optimistic_threshold",
            "type": "u64"
          }
        ]
      }
    },
    {
      "name": "UsageReported",
      "type": {
        "kind": "struct",
        "fields": [
          {
            "name": "epoch",
            "type": "pubkey"
          },
          {
            "name": "operator_bps",
            "type": "u16"
          },
          {
            "name": "challenge_deadline",
            "type": "i64"
          }
        ]
      }
    },
    {
      "name": "Verdict",
      "docs": [
        "StatusQuo means \"as if there had been no dispute\" (FR-027a). The escrow must be able to",
        "distribute funds under this outcome, otherwise auto-escalation leads nowhere."
      ],
      "type": {
        "kind": "enum",
        "variants": [
          {
            "name": "Claimant"
          },
          {
            "name": "Respondent"
          },
          {
            "name": "StatusQuo"
          }
        ]
      }
    }
  ]
} satisfies Idl
