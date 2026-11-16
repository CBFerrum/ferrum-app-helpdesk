//! Helpdesk's handle on the vault (Ferrum docs/operations.md, "The vault"). The secret in the mail sent about a ticket
//! (its subject tag and Reply-To) is made from the ticket with a key derived from Helpdesk's own key in the workspace,
//! kept nowhere; the ticket is found again by a hash of it (`inbound.rs`).

use ferrum_kernel::vault::{Scope, Vault};
use ferrum_kernel::{Error, Kernel, Result};
use std::sync::RwLock;
use uuid::Uuid;

/// Helpdesk's handle on the vault, from its registration.
static VAULT: RwLock<Option<Vault>> = RwLock::new(None);

/// Keeps the handle the registration gave Helpdesk.
// ponytail: the last registration's handle serves the whole process (`ferrum start` sets the server up twice and serves
// with the second); keep it in what is registered if one process ever serves two kernels at once.
pub(crate) fn remember(v: Vault) {
    *VAULT.write().unwrap_or_else(|e| e.into_inner()) = Some(v);
}

/// The secret of the mail sent about `ticket`: 32 hexadecimal digits of a key derived for that ticket from Helpdesk's
/// key in the workspace (the same each time while that key's version stands; another after it is rotated, while mail
/// sent before keeps finding the ticket by its own). None while the vault is closed (said in the log): the mail goes
/// without it, and a reply is matched by the mail it answers, as on a server with no key at all.
pub(crate) async fn mail_token(k: &Kernel, tenant: Uuid, ticket: Uuid) -> Result<Option<String>> {
    let v =
        VAULT.read().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| Error::Unavailable("Helpdesk was registered without its vault handle".into()))?;
    match v.derive(k, Scope::Workspace(tenant), &format!("mail-token:{ticket}")).await {
        Ok(key) => Ok(Some(key.bytes()[..16].iter().map(|b| format!("{b:02x}")).collect())),
        Err(Error::Unavailable(why)) => {
            tracing::warn!(%ticket, "the mail about a ticket goes without its secret: {why}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
