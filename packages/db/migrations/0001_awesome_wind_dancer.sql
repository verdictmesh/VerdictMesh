CREATE TABLE "settlements" (
	"dispute_pda" text PRIMARY KEY NOT NULL,
	"escrow_program" text NOT NULL,
	"signature" text NOT NULL,
	"slot" bigint NOT NULL,
	"found_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "settlements_slot_non_negative" CHECK ("settlements"."slot" >= 0)
);
--> statement-breakpoint
ALTER TABLE "settlements" ENABLE ROW LEVEL SECURITY;--> statement-breakpoint
ALTER TABLE "settlements" ADD CONSTRAINT "settlements_dispute_pda_disputes_pda_fk" FOREIGN KEY ("dispute_pda") REFERENCES "public"."disputes"("pda") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE POLICY "settlements_deny_all" ON "settlements" AS RESTRICTIVE FOR ALL TO "anon", "authenticated" USING (false) WITH CHECK (false);