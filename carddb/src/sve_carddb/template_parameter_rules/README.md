# Current parameter rules

`current` reads the registered matcher switches from the current working tree or an
explicit bounded local YAML file. Empty notes may be omitted and read as empty
strings. An empty selection enables no rules. Rule IDs,
roles and match conditions belong to the installed program; there are no historical
policy/approval pairs, producer pins or replay comparisons.

`template_parameters.candidate_matching.classify()` applies enabled rules directly
and returns slots with types, roles, code-point intervals, values, reference targets
and unmatched reasons. A new matcher cannot claim an already-owned numeric position.
Disabled numeric rules report `numeric_rule_disabled`; ambiguous and unsupported
positions keep their specific reasons. There is no second resolution step or
per-slot hash wrapper. Full-field coverage and placeholder checks still apply.
