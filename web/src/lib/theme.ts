export const themes = [
	{ id: 'blue', name: '海盐蓝', color: '#2563eb' },
	{ id: 'violet', name: '鸢尾紫', color: '#7c3aed' },
	{ id: 'rose', name: '蔷薇红', color: '#be185d' },
	{ id: 'amber', name: '琥珀橙', color: '#b45309' },
	{ id: 'teal', name: '湖水青', color: '#0f766e' },
	{ id: 'slate', name: '石墨灰', color: '#475569' }
];
export function getTheme(): string {
	try {
		const id = localStorage.getItem('bili-theme');
		return themes.some((t) => t.id === id) ? id! : 'blue';
	} catch {
		return 'blue';
	}
}
export function applyTheme(id: string) {
	const theme = themes.find((t) => t.id === id) ?? themes[0];
	document.documentElement.style.setProperty('--brand', theme.color);
	try {
		localStorage.setItem('bili-theme', theme.id);
	} catch {
		/* Private browser storage can be unavailable. */
	}
}
