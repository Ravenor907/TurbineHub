// Turbine Hub's mark for the Quick Access menu: a hub with six turbine blades, drawn in the current text color so
// it follows Steam's own icons.

const BLADES = [0, 60, 120, 180, 240, 300];

export function HubIcon(props: { size?: string }) {
  const size = props.size || "1em";
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      {BLADES.map((turn) => (
        <path key={turn} d="M12 9.2c-.9-2.3-.5-4.9 1.3-7 1.6 2.4 1.8 5 .5 7.3z" transform={"rotate(" + turn + " 12 12)"} />
      ))}
      <circle cx="12" cy="12" r="2.6" />
    </svg>
  );
}
