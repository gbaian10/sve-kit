# Current restriction list observations

`parse_restriction_list` reads the JP and EN current-list HTML layouts. It returns
Standard entries with the displayed name, linked card number (if present), raw
link, copy limit, effective dates and source heading. Lift dates are exclusive.
EN lift dates may appear beside one card link as `(to be lifted on Month D, YYYY)`.
They apply only to that card; unsupported annotations, conflicting heading dates
and ambiguous multi-card paragraphs fail closed.
It reports the intentionally excluded history/Cross Craft/Gloryfinder sections.
It never follows links, downloads images or treats another format as Standard.

The parser requires at least one entry in each ban and limit category and under
each effective-date heading, supported date headings and regional official entry
links. Unsupported structure, missing dates, invalid calendar dates, repeated
names/card numbers and line breaks inside names fail. A genuinely empty category
needs an explicitly supported layout rather than an inferred empty result.
This is an adapter for these layouts,
not a generic interpretation of arbitrary official announcements or proof of
complete historical coverage. A terminal HTML marker rejects obvious truncation.

JP display quotation marks are removed from entry names. EN surrounding card-number
query whitespace is stripped, while the original link is retained. Announcement
links provide no card number; the parser does not infer one from an image or a
regional identifier. Locators identify a one-based ordinal in the document-order
`h2,h3,h4,p,a[href]` selection under `.sw-Lower_Container`, bound by the caller to
the exact raw hash and sealed source version.

Every result is `semantic_state="unreviewed"`. Matching names to regional
`rules_name` identities, physical faces and deck scopes, confirming profiles and
adopting date coverage require explicit review. The parser supplies no approval,
authored writer, UTC knowledge cutoff or public snapshot. Existing construction
adoption gates remain closed; successful parsing does not authorize adoption.

`parse_jp_restriction_lifts` separately observes the explicit Standard lift block
in the supported JP notice layout. It preserves the linked card identity and
exclusive end date, without interpreting ban additions, evolution exceptions or
other formats. Human adoption reconciles each lift with its original list entry.
