//! A made-up studio's task list, so the board can be shown, tested and
//! photographed without anyone's real work. The same `now` always builds the
//! same world.

use crate::clock::{DAY, HOUR, Zone};
use crate::model::{Rules, Snapshot};
use crate::task::{Annotation, Status, Task};
use std::collections::HashSet;

pub fn rules() -> Rules {
    Rules::default()
}

/// A small, repeatable random source (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }

    fn uuid(&mut self) -> String {
        let (a, b) = (self.next(), self.next());
        format!(
            "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
            a >> 32,
            a & 0xffff,
            (a >> 16) & 0xfff,
            b & 0xfff,
            b >> 16
        )
    }

    fn sha(&mut self) -> String {
        format!("{:07x}", (self.next() >> 20) & 0xfff_ffff | 0xa00_0000)
    }
}

const PROJECTS: [(&str, u64, &[&str]); 7] = [
    (
        "storefront",
        9,
        &[
            "the cart drawer",
            "coupon codes",
            "the order page",
            "search filters",
            "the wishlist",
            "stock badges",
            "gift notes",
            "the size chart",
        ],
    ),
    (
        "ledger-api",
        7,
        &[
            "the audit trail",
            "currency rounding",
            "webhook retries",
            "the invoice export",
            "idempotency keys",
            "rate limits",
            "the balance cache",
            "ledger snapshots",
        ],
    ),
    (
        "moonlight",
        6,
        &[
            "the lantern glow",
            "moth pathing",
            "the save shrine",
            "bell chimes",
            "the fog volume",
            "camera easing",
            "the title card",
            "footstep sounds",
        ],
    ),
    (
        "atlas",
        4,
        &[
            "river labels",
            "the hillshade pass",
            "tile seams",
            "the legend",
            "city markers",
            "contour lines",
            "the zoom ramp",
            "ocean depth tints",
        ],
    ),
    (
        "docs-site",
        3,
        &[
            "the install page",
            "search ranking",
            "code tabs",
            "the changelog feed",
            "dark mode",
            "broken anchors",
        ],
    ),
    (
        "android",
        2,
        &[
            "the splash screen",
            "back gestures",
            "the share sheet",
            "offline mode",
        ],
    ),
    (
        "homelab",
        1,
        &[
            "the backup job",
            "disk alerts",
            "the media share",
            "DNS overrides",
        ],
    ),
];

const VERBS: [&str; 14] = [
    "Wire", "Fix", "Add", "Split", "Cache", "Trim", "Port", "Draw", "Tune", "Record", "Index",
    "Guard", "Batch", "Rework",
];

const NEXT: [&str; 8] = [
    "parser next",
    "tests next",
    "docs next",
    "the migration next",
    "review next",
    "cleanup next",
    "the empty state next",
    "error paths next",
];

fn visit(rng: &mut Rng) -> String {
    let sha = rng.sha();
    match rng.below(8) {
        0 => format!("split the work; {} ({sha})", rng.pick(&NEXT)),
        1 => format!("tests pass, {} of them ({sha})", 40 + rng.below(900)),
        2 => format!("first pass in {sha}; {}", rng.pick(&NEXT)),
        3 => "found the cause: a stale cache key".into(),
        4 => format!("review notes addressed ({sha})"),
        5 => "blocked on a flaky test; retried and it held".into(),
        6 => format!("measured before and after: 23% faster ({sha})"),
        _ => format!("done and pushed ({sha})"),
    }
}

/// A moment on local day `day`, inside working hours: mostly 8 to 17, some
/// evenings.
fn working_moment(rng: &mut Rng, day: i64, offset: i64) -> i64 {
    let hour = if rng.below(4) == 0 {
        19 + rng.below(3) as i64
    } else {
        8 + rng.below(9) as i64
    };
    day * DAY - offset + hour * HOUR + rng.below(3_600) as i64
}

fn blank(rng: &mut Rng, project: &str, title: &str, entry: i64) -> Task {
    Task {
        uuid: rng.uuid(),
        description: title.into(),
        status: Status::Pending,
        project: Some(project.into()),
        priority: Some("M".into()),
        tags: vec![],
        depends: vec![],
        annotations: vec![],
        entry,
        start: None,
        end: None,
        modified: None,
        wait: None,
        due: None,
        urgency: 4.0 + rng.below(60) as f64 / 10.0,
        reference: None,
    }
}

/// Everything in the made-up list. `offset` is the zone's seconds east of
/// UTC, so the work lands in that zone's working hours.
pub fn tasks(now: i64, offset: i64) -> Vec<Task> {
    let mut rng = Rng(0x5eed_cafe_f00d_1234);
    let today = (now + offset).div_euclid(DAY);
    let mut out: Vec<Task> = Vec::new();
    let weight: u64 = PROJECTS.iter().map(|p| p.1).sum();

    // Six weeks of finished work: quieter weekends, a busy stretch in the
    // middle, and whatever today has seen so far.
    for back in (0..42i64).rev() {
        let day = today - back;
        let weekend = matches!(crate::clock::weekday(day), 5 | 6);
        let surge = if (16..24).contains(&back) { 14 } else { 0 };
        let mut count = 9 + rng.below(12) + surge + (42 - back as u64) / 7;
        if weekend {
            count /= 3;
        }
        for _ in 0..count {
            let end = working_moment(&mut rng, day, offset);
            if end >= now {
                continue;
            }
            let mut roll = rng.below(weight);
            let (project, _, things) = PROJECTS
                .iter()
                .find(|p| {
                    if roll < p.1 {
                        true
                    } else {
                        roll -= p.1;
                        false
                    }
                })
                .unwrap_or(&PROJECTS[0]);
            let title = format!("{} {}", rng.pick(&VERBS), rng.pick(things));
            let lived = match rng.below(10) {
                0..=3 => 600 + rng.below(3 * 3_600) as i64,
                4..=6 => 4 * HOUR + rng.below(18 * 3_600) as i64,
                7..=8 => DAY + rng.below(2 * 86_400) as i64,
                _ => 4 * DAY + rng.below(6 * 86_400) as i64,
            };
            let mut t = blank(&mut rng, project, &title, end - lived);
            t.status = Status::Completed;
            t.end = Some(end);
            t.priority = Some(rng.pick(&["H", "M", "M", "M", "L"]).into());
            let decision = rng.below(14) == 0;
            if decision {
                t.description = format!("Decide {}", rng.pick(things));
                t.tags = vec!["decision".into()];
            } else if rng.below(7) == 0 {
                t.tags = vec!["bug".into()];
            }
            let visits = 1 + rng.below(4) as i64;
            for v in 0..visits {
                // Each visit falls in working hours on its own day, never
                // after the task closed.
                let rough = end - lived * (visits - 1 - v) / visits.max(1) / 2;
                let at = if v == visits - 1 {
                    end
                } else {
                    working_moment(&mut rng, (rough + offset).div_euclid(DAY), offset).min(end)
                };
                t.annotations.push(Annotation {
                    entry: at,
                    description: if decision && v == visits - 1 {
                        "Me: the simple version; revisit after launch".into()
                    } else {
                        visit(&mut rng)
                    },
                });
            }
            out.push(t);
        }
    }

    // The open list, written by hand so every lane and line has something
    // worth looking at.
    let add = |rng: &mut Rng,
               out: &mut Vec<Task>,
               project: &str,
               title: &str,
               priority: &str,
               tags: &[&str],
               after: &[usize],
               age_hours: i64|
     -> usize {
        let mut t = blank(rng, project, title, now - age_hours * HOUR);
        t.priority = Some(priority.into());
        t.tags = tags.iter().map(|s| s.to_string()).collect();
        t.depends = after.iter().map(|&i| out[i].uuid.clone()).collect();
        t.urgency = match priority {
            "H" => 9.0,
            "M" => 6.0,
            _ => 3.0,
        } + rng.below(20) as f64 / 10.0;
        let visits = rng.below(3);
        for v in 0..visits {
            t.annotations.push(Annotation {
                entry: now - (age_hours * HOUR) * (visits - v) as i64 / (visits + 1) as i64,
                description: visit(rng),
            });
        }
        out.push(t);
        out.len() - 1
    };
    let r = &mut rng;
    let o = &mut out;

    // The Checkout line: a trunk, a branch, and two stops that need a person.
    let cart = add(
        r,
        o,
        "storefront",
        "Checkout: cart model",
        "M",
        &[],
        &[],
        90,
    );
    o[cart].status = Status::Completed;
    o[cart].end = Some(now - 3 * HOUR);
    let sheet = add(
        r,
        o,
        "storefront",
        "Checkout: payment sheet",
        "H",
        &[],
        &[cart],
        80,
    );
    o[sheet].start = Some(now - 47 * 60);
    let receipts = add(
        r,
        o,
        "storefront",
        "Checkout: receipts by email",
        "M",
        &[],
        &[sheet],
        80,
    );
    let tax = add(
        r,
        o,
        "storefront",
        "Checkout: tax tables",
        "M",
        &[],
        &[sheet],
        70,
    );
    let phone = add(
        r,
        o,
        "storefront",
        "Checkout: playtest on a real phone",
        "H",
        &["me"],
        &[receipts],
        70,
    );
    let flag = add(
        r,
        o,
        "storefront",
        "Checkout: launch behind a flag",
        "M",
        &[],
        &[phone, tax],
        60,
    );
    add(
        r,
        o,
        "storefront",
        "Checkout: announce to the list",
        "L",
        &["me"],
        &[flag],
        60,
    );

    // One playtest holds three builds.
    let lantern = add(
        r,
        o,
        "moonlight",
        "Playtest the lantern level",
        "H",
        &["me"],
        &[],
        200,
    );
    add(
        r,
        o,
        "moonlight",
        "Build the moth swarm",
        "M",
        &[],
        &[lantern],
        150,
    );
    add(
        r,
        o,
        "moonlight",
        "Build the bell puzzle",
        "M",
        &[],
        &[lantern],
        150,
    );
    add(
        r,
        o,
        "moonlight",
        "Tune the lantern falloff",
        "L",
        &[],
        &[lantern],
        140,
    );

    let legend = add(
        r,
        o,
        "atlas",
        "Approve the map legend",
        "H",
        &["me"],
        &[],
        50,
    );
    let tiles = add(
        r,
        o,
        "atlas",
        "Render the regional tiles",
        "M",
        &[],
        &[legend],
        48,
    );
    add(
        r,
        o,
        "atlas",
        "Publish the atlas preview",
        "M",
        &[],
        &[tiles],
        48,
    );

    let key = add(
        r,
        o,
        "android",
        "Sign the release key",
        "M",
        &["me"],
        &[],
        120,
    );
    add(
        r,
        o,
        "android",
        "Upload to the internal track",
        "M",
        &[],
        &[key],
        110,
    );

    // Started work.
    for (project, title, priority, mins) in [
        ("ledger-api", "Batch the invoice export", "M", 12),
        ("moonlight", "Rework the save shrine prompt", "H", 95),
        ("docs-site", "Fix broken anchors in the guide", "M", 31),
    ] {
        let i = add(r, o, project, title, priority, &[], &[], 30);
        o[i].start = Some(now - mins * 60);
    }
    // The user's own desk.
    for (project, title, priority, tags) in [
        ("moonlight", "Record the bell chime foley", "M", &["me"][..]),
        (
            "storefront",
            "Pick the launch discount",
            "M",
            &["me", "decision"][..],
        ),
        (
            "ledger-api",
            "Choose the rounding rule for refunds",
            "H",
            &["me", "decision"][..],
        ),
        (
            "docs-site",
            "Review and publish the launch post",
            "M",
            &["me"][..],
        ),
        ("homelab", "Swap the failing backup disk", "H", &["me"][..]),
        ("android", "Claim the store listing name", "L", &["me"][..]),
        ("atlas", "Photograph the paper proofs", "L", &["me"][..]),
    ] {
        let age = 26 + r.below(200) as i64;
        add(r, o, project, title, priority, tags, &[], age);
    }
    // Ready for an agent.
    for (project, title, priority, tags) in [
        (
            "ledger-api",
            "Guard webhook retries against replays",
            "H",
            &[][..],
        ),
        (
            "storefront",
            "Fix the coupon rounding on bundles",
            "H",
            &["bug"][..],
        ),
        (
            "storefront",
            "Add stock badges to search results",
            "M",
            &[][..],
        ),
        (
            "ledger-api",
            "Index the audit trail by account",
            "M",
            &[][..],
        ),
        ("moonlight", "Tune moth pathing near water", "M", &[][..]),
        ("atlas", "Fix tile seams at zoom 9", "M", &["bug"][..]),
        (
            "docs-site",
            "Add code tabs to the install page",
            "M",
            &[][..],
        ),
        ("moonlight", "Draw the title card at 4K", "M", &[][..]),
        ("ledger-api", "Cache balances per currency", "M", &[][..]),
        (
            "docs-site",
            "Rank guides above reference in search",
            "L",
            &["research"][..],
        ),
        ("homelab", "Alert when a disk passes 85%", "L", &[][..]),
        ("android", "Port back gestures to the new API", "L", &[][..]),
        (
            "atlas",
            "Draw contour lines for the highlands",
            "L",
            &[][..],
        ),
    ] {
        let age = 5 + r.below(160) as i64;
        add(r, o, project, title, priority, tags, &[], age);
    }
    // Hidden until later.
    for (project, title) in [
        ("moonlight", "Port the fog volume to the console build"),
        ("storefront", "Rework the wishlist as shared lists"),
        ("atlas", "Tint ocean depths from the survey data"),
    ] {
        let i = add(r, o, project, title, "L", &[], &[], 300);
        o[i].wait = Some(now + 400 * DAY);
    }
    out
}

/// A note for a made-up task.
pub fn note(title: &str) -> String {
    if title == "Playtest the lantern level" {
        return "# Playtest the lantern level\n\n\
            Built last week (commits `5c0ffee..b0a7f1d`). The lantern now lights a cone, moths \
            gather at its edge, and the bell at the end of the level only rings in the dark.\n\n\
            Things to judge by hand:\n\
            - [x] Does the first dark corridor read as safe to enter?\n\
            - [x] Is the cone wide enough to see the ledge before the jump?\n\
            - [ ] Do the moths point at the hidden path, or just look pretty?\n\
            - [ ] Is the bell fair on the first try?\n\
            - [ ] A full run should take 4 to 6 minutes. If it runs long, lower `lantern.drain` \
            in `data/level-3.json`.\n\n\
            ## Debug keys\n\n```\nL  refill the lantern\nB  ring the bell\n0  restart the level\n```\n"
            .into();
    }
    if title == "Pick the launch discount" {
        return "# Pick the launch discount\n\nLaunch is Friday. The list has 4,100 people on it and \
            the last sale lifted orders for nine days.\n\n## Options\n\n- 10% for the first week\n\
            - Free shipping, no discount\n- No offer at launch\n"
            .into();
    }
    if title == "Choose the rounding rule for refunds" {
        return "# Choose the rounding rule for refunds\n\nA refund split across three items can \
            be a cent off the charge. Either rule is two lines in `ledger/round.rs`.\n\n\
            ## Options\n\n- Round half to even\n- Give the odd cent to the largest line\n"
            .into();
    }
    format!(
        "# {title}\n\nWhat this is for, where the code lives and what done looks like. A note \
         carries whatever the card title cannot.\n\n- Start in `src/` and follow the marker.\n- \
         Done when the tests pass and the change is pushed.\n"
    )
}

/// The made-up world as a snapshot.
pub fn snapshot(now: i64, zone: Zone) -> Snapshot {
    let tasks = tasks(now, zone.offset(now));
    let noted: HashSet<String> = tasks
        .iter()
        .filter(|t| t.open())
        .map(|t| t.uuid.clone())
        .collect();
    crate::model::build(&tasks, &noted, &rules(), now, zone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Lane;
    use crate::model::tests::NOW;

    #[test]
    fn the_demo_fills_every_view() {
        let zone = Zone::Fixed(-6 * HOUR);
        let s = snapshot(NOW, zone);
        assert_eq!(
            s,
            snapshot(NOW, zone),
            "the same moment builds the same world"
        );
        for lane in [
            Lane::Ready,
            Lane::Doing,
            Lane::Yours,
            Lane::Held,
            Lane::Parked,
            Lane::Done,
        ] {
            assert!(!s.lane(lane).is_empty(), "{lane:?} is empty");
        }
        let checkout = s
            .lines
            .iter()
            .find(|l| l.name == "Checkout")
            .expect("the Checkout line");
        assert_eq!((checkout.cols, checkout.tracks), (6, 2));
        assert_eq!(s.lines.len(), 4);
        let lantern = s
            .cards
            .iter()
            .find(|c| c.title == "Playtest the lantern level")
            .unwrap();
        assert_eq!(lantern.frees.len(), 3);
        assert_eq!(
            s.lane(Lane::Yours)[0].frees.len(),
            3,
            "what frees the most comes first"
        );
        assert!(s.pulse.days.iter().all(|d| d.done > 0 || d.weekday >= 5));
        assert!(s.pulse.commit_share > 0.4);
        assert!(s.pulse.decision_hours.is_some());
        assert!(note(&lantern.title).contains("- [ ] Is the bell fair"));
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Snapshot>(&json).unwrap(), s);
    }
}
