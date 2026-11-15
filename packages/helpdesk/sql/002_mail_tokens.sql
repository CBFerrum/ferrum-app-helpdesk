-- Helpdesk 0.12: the secret in the mail sent about a ticket is made with a key from Helpdesk's own key in the vault and
-- kept nowhere. What finds the ticket by it is a hash of the workspace and the secret, recorded here when the ticket is
-- made and as mail goes ('vault'), and, for the secrets 0.11 made with the server's key, by upkeep ('server'). A secret
-- is 128 bits: its SHA-256 gives nothing back. A copy of the workspace under another id (a sandbox) finds nothing by
-- the hashes copied with it; a workspace moved elsewhere keeps its id, and finds its tickets by them.
create table if not exists helpdesk_mail_token (
  tenant_id uuid not null references tenant(id) on delete cascade,
  token_hash text not null check (length(token_hash) = 64),
  ticket uuid not null,
  kind text not null check (kind in ('vault', 'server')),
  primary key (tenant_id, token_hash)
);
create index if not exists helpdesk_mail_token_ticket on helpdesk_mail_token (tenant_id, ticket, kind);
alter table helpdesk_mail_token enable row level security;
alter table helpdesk_mail_token force row level security;
create policy tenant_isolation on helpdesk_mail_token
  using (tenant_id = (select ferrum_tenant()) or (select ferrum_is_system()))
  with check (tenant_id = (select ferrum_tenant()) or (select ferrum_is_system()));
