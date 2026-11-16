// The battle screen styles its parts with a stylesheet of its own, so its class names are allowed
// under src/pages/play/ and nowhere else.

// case: part classes of the battle stylesheet -> none
export const Parts = () => <div className="bp-card bp-onfield" />

// case: state classes of the battle stylesheet -> none
export const States = () => <div className="is-engaged plates-right hand-row" />

// case: battle classes beside Tailwind ones -> none
export const Mixed = () => <span className="bp-life is-active sr-only" />

// case: any other unknown class is still an error -> better-tailwindcss/no-unknown-classes
export const Other = () => <div className="card-inner" />

// case: the removed palette stays removed beside a battle class -> better-tailwindcss/no-unknown-classes
export const Palette = () => <div className="bp-card bg-gray-900" />
