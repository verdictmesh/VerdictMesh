CREATE TYPE "public"."dispute_state" AS ENUM('OptimisticPending', 'Committing', 'Revealing', 'Tallied', 'Appealed', 'Finalized');--> statement-breakpoint
CREATE TYPE "public"."evidence_kind" AS ENUM('transaction', 'account');--> statement-breakpoint
CREATE TYPE "public"."verdict" AS ENUM('Claimant', 'Respondent', 'StatusQuo');--> statement-breakpoint
CREATE TABLE "disputes" (
	"pda" text PRIMARY KEY NOT NULL,
	"integrator" text NOT NULL,
	"escrow_ref" text NOT NULL,
	"claimant" text NOT NULL,
	"respondent" text NOT NULL,
	"amount" numeric(20, 0) NOT NULL,
	"state" "dispute_state" NOT NULL,
	"panel" text[] NOT NULL,
	"report_hash" char(64),
	"claimant_claim_hash" char(64) NOT NULL,
	"respondent_claim_hash" char(64) NOT NULL,
	"opened_at" bigint NOT NULL,
	"commit_deadline" bigint NOT NULL,
	"reveal_deadline" bigint NOT NULL,
	"appeal_deadline" bigint NOT NULL,
	"votes_claimant" integer NOT NULL,
	"votes_respondent" integer NOT NULL,
	"escalated" boolean NOT NULL,
	"verdict" "verdict",
	"synced_slot" bigint NOT NULL,
	CONSTRAINT "disputes_amount_non_negative" CHECK ("disputes"."amount" >= 0),
	CONSTRAINT "disputes_votes_claimant_non_negative" CHECK ("disputes"."votes_claimant" >= 0),
	CONSTRAINT "disputes_votes_respondent_non_negative" CHECK ("disputes"."votes_respondent" >= 0),
	CONSTRAINT "disputes_synced_slot_non_negative" CHECK ("disputes"."synced_slot" >= 0),
	CONSTRAINT "disputes_reveal_after_commit" CHECK ("disputes"."reveal_deadline" > "disputes"."commit_deadline"),
	CONSTRAINT "disputes_appeal_after_reveal" CHECK ("disputes"."appeal_deadline" = 0 or "disputes"."appeal_deadline" >= "disputes"."reveal_deadline"),
	CONSTRAINT "disputes_verdict_matches_state" CHECK (("disputes"."verdict" is null) = ("disputes"."state" in ('OptimisticPending', 'Committing', 'Revealing'))),
	CONSTRAINT "disputes_report_hash_lower_hex" CHECK ("disputes"."report_hash" ~ '^[0-9a-f]{64}$'),
	CONSTRAINT "disputes_claimant_claim_hash_lower_hex" CHECK ("disputes"."claimant_claim_hash" ~ '^[0-9a-f]{64}$'),
	CONSTRAINT "disputes_respondent_claim_hash_lower_hex" CHECK ("disputes"."respondent_claim_hash" ~ '^[0-9a-f]{64}$')
);
--> statement-breakpoint
ALTER TABLE "disputes" ENABLE ROW LEVEL SECURITY;--> statement-breakpoint
CREATE TABLE "evidence" (
	"dispute_pda" text NOT NULL,
	"kind" "evidence_kind" NOT NULL,
	"source" text NOT NULL,
	"slot" bigint NOT NULL,
	"payload" jsonb NOT NULL,
	"fetched_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "evidence_dispute_pda_source_pk" PRIMARY KEY("dispute_pda","source"),
	CONSTRAINT "evidence_slot_non_negative" CHECK ("evidence"."slot" >= 0)
);
--> statement-breakpoint
ALTER TABLE "evidence" ENABLE ROW LEVEL SECURITY;--> statement-breakpoint
CREATE TABLE "reports" (
	"dispute_pda" text NOT NULL,
	"version" integer NOT NULL,
	"content" jsonb NOT NULL,
	"content_hash" char(64) NOT NULL,
	"model" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "reports_dispute_pda_version_pk" PRIMARY KEY("dispute_pda","version"),
	CONSTRAINT "reports_version_positive" CHECK ("reports"."version" >= 1),
	CONSTRAINT "reports_content_hash_lower_hex" CHECK ("reports"."content_hash" ~ '^[0-9a-f]{64}$')
);
--> statement-breakpoint
ALTER TABLE "reports" ENABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "evidence" ADD CONSTRAINT "evidence_dispute_pda_disputes_pda_fk" FOREIGN KEY ("dispute_pda") REFERENCES "public"."disputes"("pda") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "reports" ADD CONSTRAINT "reports_dispute_pda_disputes_pda_fk" FOREIGN KEY ("dispute_pda") REFERENCES "public"."disputes"("pda") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "disputes_state_opened_idx" ON "disputes" USING btree ("state","opened_at");--> statement-breakpoint
CREATE INDEX "disputes_integrator_idx" ON "disputes" USING btree ("integrator");--> statement-breakpoint
CREATE INDEX "disputes_panel_idx" ON "disputes" USING gin ("panel");--> statement-breakpoint
CREATE INDEX "evidence_dispute_slot_idx" ON "evidence" USING btree ("dispute_pda","slot");--> statement-breakpoint
CREATE POLICY "disputes_deny_all" ON "disputes" AS RESTRICTIVE FOR ALL TO "anon", "authenticated" USING (false) WITH CHECK (false);--> statement-breakpoint
CREATE POLICY "evidence_deny_all" ON "evidence" AS RESTRICTIVE FOR ALL TO "anon", "authenticated" USING (false) WITH CHECK (false);--> statement-breakpoint
CREATE POLICY "reports_deny_all" ON "reports" AS RESTRICTIVE FOR ALL TO "anon", "authenticated" USING (false) WITH CHECK (false);