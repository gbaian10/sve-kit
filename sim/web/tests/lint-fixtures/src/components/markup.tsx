// case: dangerouslySetInnerHTML -> no-restricted-syntax
export const Html = ({ html }: { html: string }) => (
  <div dangerouslySetInnerHTML={{ __html: html }} />
)

// case: image without alt -> jsx-a11y/alt-text
export const NoAlt = () => <img src="/a.png" />

declare function CardImage(props: { src: string; alt?: string }): React.JSX.Element

// case: CardImage is checked as an img -> jsx-a11y/alt-text
export const CardNoAlt = () => <CardImage src="/a.png" />

// case: mouse-only click on a div -> jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions
export const MouseOnly = ({ onPick }: { onPick: () => void }) => <div onClick={onPick} />
