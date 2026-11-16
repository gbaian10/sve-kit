// The battle screen styles its parts with a stylesheet of its own. The classes that stylesheet
// defines are known under src/pages/play/ and nowhere else; a name it does not define is an error
// there too, whatever it starts with.

// case: part classes of the battle stylesheet -> none
export const Parts = () => <div className="bp-card bp-life" />

// case: state classes of the battle stylesheet -> none
export const States = () => <div className="is-engaged plates-top hand-column" />

// case: battle classes beside Tailwind ones -> none
export const Mixed = () => <span className="bp-life is-active sr-only" />

// case: a misspelt part class -> better-tailwindcss/no-unknown-classes
export const Typo = () => <div className="bp-plyer" />

// case: a state the battle stylesheet does not define -> better-tailwindcss/no-unknown-classes
export const NoSuchState = () => <div className="bp-card is-flying" />

// case: any other unknown class is still an error -> better-tailwindcss/no-unknown-classes
export const Other = () => <div className="card-inner" />

// case: the removed palette stays removed beside a battle class -> better-tailwindcss/no-unknown-classes
export const Palette = () => <div className="bp-card bg-gray-900" />
