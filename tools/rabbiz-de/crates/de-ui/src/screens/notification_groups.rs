//! Notification app-grouping and lockscreen privacy redaction helpers.

use std::collections::HashMap;

use crate::panels::Notification;

/// User preference for notification visibility on the lockscreen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LockscreenPrivacy {
    /// Show full title and body on the lockscreen.
    ShowAll,
    /// Show app name and timestamp, replacing title with a generic label and
    /// hiding body content.
    #[default]
    HideSensitive,
    /// Hide all notification cards on the lockscreen.
    HideAll,
}

/// Notifications grouped by source application.
#[derive(Debug, Clone)]
pub struct NotificationGroup {
    pub app: String,
    pub items: Vec<Notification>,
    pub expanded: bool,
}

impl NotificationGroup {
    /// Number of notifications in this group.
    #[must_use]
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Whether this group should render as a stacked header card (`count > 1`).
    #[must_use]
    pub fn is_stacked(&self) -> bool {
        self.items.len() > 1
    }
}

/// Group notifications by `app`, preserving first-seen order of groups and the
/// relative order of notifications within each group.
#[must_use]
pub fn group_notifications(entries: &[Notification]) -> Vec<NotificationGroup> {
    let mut index_by_app: HashMap<&str, usize> = HashMap::new();
    let mut groups: Vec<NotificationGroup> = Vec::new();
    for entry in entries {
        if let Some(&idx) = index_by_app.get(entry.app.as_str()) {
            groups[idx].items.push(entry.clone());
        } else {
            let idx = groups.len();
            index_by_app.insert(entry.app.as_str(), idx);
            groups.push(NotificationGroup {
                app: entry.app.clone(),
                items: vec![entry.clone()],
                expanded: false,
            });
        }
    }
    groups
}

/// Apply the lockscreen privacy policy to a notification list.
#[must_use]
pub fn redact_for_lockscreen(
    entries: &[Notification],
    privacy: LockscreenPrivacy,
    redacted_title: &str,
) -> Vec<Notification> {
    match privacy {
        LockscreenPrivacy::ShowAll => entries.to_vec(),
        LockscreenPrivacy::HideSensitive => entries
            .iter()
            .map(|entry| Notification {
                id: entry.id,
                app: entry.app.clone(),
                title: redacted_title.to_string(),
                body: String::new(),
                when: entry.when.clone(),
                icon: entry.icon.clone(),
            })
            .collect(),
        LockscreenPrivacy::HideAll => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notif(id: u32, app: &str, title: &str, body: &str, when: &str) -> Notification {
        Notification {
            id,
            app: app.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            when: when.to_string(),
            icon: None,
        }
    }

    #[test]
    fn groups_preserve_first_seen_order_and_item_order() {
        let entries = vec![
            notif(1, "Messages", "Alice", "Hey", "now"),
            notif(2, "Mail", "Invoice", "Paid", "2 min"),
            notif(3, "Messages", "Bob", "Dinner?", "3 min"),
            notif(4, "Calendar", "Standup", "In 10m", "5 min"),
            notif(5, "Messages", "Alice", "See you soon", "6 min"),
        ];

        let groups = group_notifications(&entries);
        assert_eq!(groups.len(), 3);

        assert_eq!(groups[0].app, "Messages");
        assert_eq!(groups[0].count(), 3);
        assert!(groups[0].is_stacked());
        assert!(!groups[0].expanded);
        assert_eq!(
            groups[0].items.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![1, 3, 5]
        );

        assert_eq!(groups[1].app, "Mail");
        assert_eq!(groups[1].count(), 1);
        assert!(!groups[1].is_stacked());
        assert_eq!(groups[1].items[0].id, 2);

        assert_eq!(groups[2].app, "Calendar");
        assert_eq!(groups[2].count(), 1);
        assert!(!groups[2].is_stacked());
        assert_eq!(groups[2].items[0].id, 4);
    }

    #[test]
    fn empty_input_produces_no_groups() {
        assert!(group_notifications(&[]).is_empty());
    }

    #[test]
    fn lockscreen_redaction_handles_all_three_privacy_modes() {
        let entries = vec![
            notif(10, "Messages", "Alice", "Secret OTP: 123456", "now"),
            notif(11, "Bank", "Transfer", "$500 received", "4 min"),
        ];

        let show_all =
            redact_for_lockscreen(&entries, LockscreenPrivacy::ShowAll, "New notification");
        assert_eq!(show_all.len(), 2);
        assert_eq!(show_all[0].title, "Alice");
        assert_eq!(show_all[0].body, "Secret OTP: 123456");
        assert_eq!(show_all[1].title, "Transfer");
        assert_eq!(show_all[1].body, "$500 received");

        let hide_sensitive = redact_for_lockscreen(
            &entries,
            LockscreenPrivacy::HideSensitive,
            "New notification",
        );
        assert_eq!(hide_sensitive.len(), 2);
        assert_eq!(hide_sensitive[0].id, 10);
        assert_eq!(hide_sensitive[0].app, "Messages");
        assert_eq!(hide_sensitive[0].when, "now");
        assert_eq!(hide_sensitive[0].title, "New notification");
        assert_eq!(hide_sensitive[0].body, "");
        assert_eq!(hide_sensitive[1].id, 11);
        assert_eq!(hide_sensitive[1].app, "Bank");
        assert_eq!(hide_sensitive[1].when, "4 min");
        assert_eq!(hide_sensitive[1].title, "New notification");
        assert_eq!(hide_sensitive[1].body, "");

        let hide_all =
            redact_for_lockscreen(&entries, LockscreenPrivacy::HideAll, "New notification");
        assert!(hide_all.is_empty());
    }
}
