import { accents, materials } from '$lib/appearance/presets';
import { writable } from 'svelte/store';

export { accents, materials };
export type Accent = (typeof accents)[number]['id'];
export type Material = (typeof materials)[number]['id'];
type Appearance = { accent: Accent; material: Material };
const defaults: Appearance = { accent: 'pink', material: 'default' };
export const appearance = writable<Appearance>(defaults);

function read(key: string): string | null {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
}
function save(key: string, value: string) {
	try {
		localStorage.setItem(key, value);
	} catch {
		/* Selection still works in this session. */
	}
}
function validAccent(value: string | null): Accent {
	return accents.find((item) => item.id === value)?.id ?? 'pink';
}
function validMaterial(value: string | null): Material {
	return (
		materials.find((item) => item.id === (value === 'blocks' ? 'satin' : value))?.id ?? 'default'
	);
}
function ink(color: string): string {
	const channels = color
		.slice(1)
		.match(/.{2}/g)!
		.map((hex) => {
			const value = parseInt(hex, 16) / 255;
			return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
		});
	const luminance = channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
	return 1.05 / (luminance + 0.05) >= 4.5 ? '#ffffff' : '#17202e';
}
function apply(next: Appearance) {
	if (typeof document === 'undefined') return;
	const palette = accents.find((item) => item.id === next.accent)!;
	const root = document.documentElement;
	root.dataset.accent = next.accent;
	root.dataset.material = next.material;
	root.dataset.palette = 'secondary' in palette ? 'multi' : 'single';
	const multi = 'secondary' in palette;
	const values = {
		'--accent-light': palette.light,
		'--accent-dark': palette.dark,
		'--accent-light-ink': ink(palette.light),
		'--accent-dark-ink': ink(palette.dark),
		'--secondary-light': multi ? palette.secondary : palette.light,
		'--secondary-dark': multi ? palette.secondaryDark : palette.dark,
		'--tertiary-light': multi ? palette.tertiary : palette.light,
		'--tertiary-dark': multi ? palette.tertiaryDark : palette.dark
	};
	for (const [name, value] of Object.entries(values)) root.style.setProperty(name, value);
	appearance.set(next);
}
export function setAccent(accent: Accent) {
	const material = validMaterial(document.documentElement.dataset.material ?? null);
	save('bili-theme', accent);
	apply({ accent: validAccent(accent), material });
}
export function setMaterial(material: Material) {
	const accent = validAccent(document.documentElement.dataset.accent ?? null);
	save('bili-material', material);
	apply({ accent, material: validMaterial(material) });
}
export function initializeAppearance() {
	const restore = () =>
		apply({
			accent: validAccent(read('bili-theme')),
			material: validMaterial(read('bili-material'))
		});
	restore();
	const sync = (event: StorageEvent) => {
		if (event.key === null || event.key === 'bili-theme' || event.key === 'bili-material')
			restore();
	};
	window.addEventListener('storage', sync);
	return () => window.removeEventListener('storage', sync);
}
