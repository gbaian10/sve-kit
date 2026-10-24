# Current restriction list observations

`parse_restriction_list` reads the JP and EN current-list HTML layouts. It returns
Standard entries with the displayed name, linked card number (if present), raw
link, copy limit, effective dates and source heading. Lift dates are exclusive.
It reports the intentionally excluded history/Cross Craft/Gloryfinder sections.
It never follows links, downloads images or treats another format as Standard.

The parser requires both ban and limit categories, supported date headings and
regional official entry links. Unsupported structure, missing dates, invalid
calendar dates and duplicate entries fail. This is an adapter for these layouts,
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
