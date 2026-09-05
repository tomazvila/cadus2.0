//! The account behind one federated sign-in: the section 3.3 resolution walk,
//! and the one transaction that writes the sign-in.

use cadus_store::Db;
use cadus_store::auth::{
    AuthUser, NewSession, SignUp, clear_password_hash, delete_all_sessions, insert_oauth_account,
    insert_session, mark_email_verified, oauth_account_user, sign_up, user_by_email, user_by_id,
};

use crate::auth::oauth::Identity;
use crate::auth::routes::{bind, commit};
use crate::auth::store_call;
use crate::error::ApiError;

/// The writes of one federated sign-in, in ONE transaction: the first-sign-in
/// stamp, the link row, and the session row.
///
/// The stamp removes the ONE reason login refuses a password on an unverified
/// account, so every credential that predates the stamp goes first. Sign-up
/// asks for no proof of the address, so the password and the sessions of an
/// unverified account have an unproven source. The order is fixed — clear,
/// delete, then stamp — and the three writes share this transaction, so no
/// window opens in which the address is verified and the old password still
/// signs in. The session row of THIS sign-in goes in last, after the deletion.
pub(super) async fn write_sign_in(
    db: &Db,
    user: &AuthUser,
    linked: bool,
    identity: &Identity,
    session: &NewSession<'_>,
) -> Result<(), ApiError> {
    let mut tx = bind(db, user.id).await?;
    if user.email_verified_at.is_none() {
        store_call(db, "password clear", clear_password_hash(&mut *tx, user.id)).await?;
        store_call(db, "session sweep", delete_all_sessions(&mut *tx)).await?;
        // The provider just proved the address, which is what the verification
        // link proves.
        store_call(
            db,
            "verification stamp",
            mark_email_verified(&mut *tx, user.id),
        )
        .await?;
    }
    if !linked {
        store_call(
            db,
            "oauth link",
            insert_oauth_account(
                &mut *tx,
                user.id,
                &identity.provider,
                &identity.subject,
                &identity.email,
            ),
        )
        .await?;
    }
    store_call(
        db,
        "session insert",
        insert_session(&mut *tx, user.id, session),
    )
    .await?;
    commit(db, tx, "session insert").await
}

/// Resolve the identity to an account, in the section 3.3 order.
///
/// The answer is the account and whether step 1 already found the
/// `oauth_accounts` row. A `true` there means the caller writes no link row: the
/// table's primary key is `(provider, provider_account_id)`, so a second insert
/// of a live link is a duplicate-key error, and re-pointing a live link at
/// another account is exactly the takeover this order prevents.
///
/// The guard on `disabled_at` belongs to the CALLER, not here. A resolution
/// branch that also tests usability is a match condition, and a non-match then
/// falls through into the next branch instead of refusing.
pub(super) async fn resolve_account(
    db: &Db,
    identity: &Identity,
) -> Result<(AuthUser, bool), ApiError> {
    let linked = store_call(
        db,
        "oauth lookup",
        oauth_account_user(db.pool(), &identity.provider, &identity.subject),
    )
    .await?;
    if let Some(user_id) = linked {
        // A dangling link — the account row is gone — resolves to nothing, and
        // the walk goes on. That is the ONLY reason step 1 continues.
        if let Some(user) = store_call(db, "account lookup", user_by_id(db.pool(), user_id)).await?
        {
            return Ok((user, true));
        }
    }

    if let Some(user) = store_call(
        db,
        "account lookup",
        user_by_email(db.pool(), &identity.email),
    )
    .await?
    {
        return Ok((user, false));
    }

    // A brand-new federated account: no password hash, so a password login
    // against it is refused by the section 3.3 login order.
    match store_call(db, "sign-up", sign_up(db.pool(), &identity.email, None)).await? {
        SignUp::Created(user) => Ok((user, false)),
        // Another request created the same address between the read above and
        // this insert. Read it back and link into it instead of failing.
        SignUp::EmailTaken => store_call(
            db,
            "account lookup",
            user_by_email(db.pool(), &identity.email),
        )
        .await?
        .map(|user| (user, false))
        .ok_or_else(|| {
            tracing::error!("auth: the OAuth sign-up raced and the address then read back empty");
            ApiError::internal("sign-up")
        }),
    }
}
