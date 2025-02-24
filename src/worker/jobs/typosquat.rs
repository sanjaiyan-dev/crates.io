use async_trait::async_trait;
use std::sync::Arc;

use crates_io_worker::BackgroundJob;
use diesel::PgConnection;
use typomania::Package;

use crate::email::Email;
use crate::tasks::spawn_blocking;
use crate::{
    typosquat::{Cache, Crate},
    worker::Environment,
    Emails,
};

/// A job to check the name of a newly published crate against the most popular crates to see if
/// the new crate might be typosquatting an existing, popular crate.
#[derive(Serialize, Deserialize, Debug)]
pub struct CheckTyposquat {
    name: String,
}

impl CheckTyposquat {
    pub fn new(name: &str) -> Self {
        Self { name: name.into() }
    }
}

#[async_trait]
impl BackgroundJob for CheckTyposquat {
    const JOB_NAME: &'static str = "check_typosquat";

    type Context = Arc<Environment>;

    #[instrument(skip(env), err)]
    async fn run(&self, env: Self::Context) -> anyhow::Result<()> {
        let crate_name = self.name.clone();

        spawn_blocking(move || {
            let mut conn = env.connection_pool.get()?;
            let cache = env.typosquat_cache(&mut conn)?;
            check(&env.emails, cache, &mut conn, &crate_name)
        })
        .await
    }
}

fn check(
    emails: &Emails,
    cache: &Cache,
    conn: &mut PgConnection,
    name: &str,
) -> anyhow::Result<()> {
    if let Some(harness) = cache.get_harness() {
        info!(name, "Checking new crate for potential typosquatting");

        let krate: Box<dyn Package> = Box::new(Crate::from_name(conn, name)?);
        let squats = harness.check_package(name, krate)?;
        if !squats.is_empty() {
            // Well, well, well. For now, the only action we'll take is to e-mail people who
            // hopefully care to check into things more closely.
            info!(?squats, "Found potential typosquatting");

            let email = PossibleTyposquatEmail {
                domain: &emails.domain,
                crate_name: name,
                squats: &squats,
            };

            for recipient in cache.iter_emails() {
                if let Err(error) = emails.send(recipient, email.clone()) {
                    error!(
                        ?error,
                        ?recipient,
                        "Failed to send possible typosquat notification"
                    );
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct PossibleTyposquatEmail<'a> {
    domain: &'a str,
    crate_name: &'a str,
    squats: &'a [typomania::checks::Squat],
}

impl Email for PossibleTyposquatEmail<'_> {
    const SUBJECT: &'static str = "Possible typosquatting in new crate";

    fn body(&self) -> String {
        let squats = self
            .squats
            .iter()
            .map(|squat| {
                let domain = self.domain;
                let crate_name = squat.package();
                format!("- {squat} (https://{domain}/crates/{crate_name})\n")
            })
            .collect::<Vec<_>>()
            .join("");

        format!(
            "New crate {crate_name} may be typosquatting one or more other crates.\n
Visit https://{domain}/crates/{crate_name} to see the offending crate.\n
\n
Specific squat checks that triggered:\n
\n
{squats}",
            domain = self.domain,
            crate_name = self.crate_name,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{test_util::pg_connection, typosquat::test_util::Faker};
    use lettre::Address;

    use super::*;

    #[test]
    fn integration() -> anyhow::Result<()> {
        let emails = Emails::new_in_memory();
        let mut faker = Faker::new(pg_connection());

        // Set up a user and a popular crate to match against.
        let user = faker.user("a")?;
        faker.crate_and_version("my-crate", "It's awesome", &user, 100)?;

        // Prime the cache so it only includes the crate we just created.
        let cache = Cache::new(vec!["admin@example.com".to_string()], faker.borrow_conn())?;

        // Now we'll create new crates: one problematic, one not so.
        let other_user = faker.user("b")?;
        let (angel, _version) = faker.crate_and_version(
            "innocent-crate",
            "I'm just a simple, innocent crate",
            &other_user,
            0,
        )?;
        let (demon, _version) = faker.crate_and_version(
            "mycrate",
            "I'm even more innocent, obviously",
            &other_user,
            0,
        )?;

        // OK, we're done faking stuff.
        let mut conn = faker.into_conn();

        // Run the check with a crate that shouldn't cause problems.
        check(&emails, &cache, &mut conn, &angel.name)?;
        assert!(emails.mails_in_memory().unwrap().is_empty());

        // Now run the check with a less innocent crate.
        check(&emails, &cache, &mut conn, &demon.name)?;
        let sent_mail = emails.mails_in_memory().unwrap();
        assert!(!sent_mail.is_empty());
        let sent = sent_mail.into_iter().next().unwrap();
        assert_eq!(&sent.0.to(), &["admin@example.com".parse::<Address>()?]);

        Ok(())
    }
}
