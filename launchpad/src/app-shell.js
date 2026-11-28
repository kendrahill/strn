import logo from './saturn-logo.png';
import icons from './terminal-icons.svg';
import './app-shell.css';

const destinations = [
  { path: '/', label: 'Home', icon: 'layout-grid' },
  { path: '/#mint', label: 'Genesis', icon: 'orbit' },
  { path: '/launchpad/', label: 'Launchpad', icon: 'zap' },
  { path: '/launchpad/testnet/', label: 'Testnet', icon: 'activity' },
  { path: '/docs/', label: 'Docs', icon: 'book-open' },
];

const page = window.location.pathname.replace(/(?:\/index\.html)?\/?$/, '/');
const preview = page === '/launchpad/testnet/' && new URLSearchParams(window.location.search).get('preview') === '1';

class SaturnShell extends HTMLElement {
  connectedCallback() {
    if (preview) return;
    document.body.classList.add('saturn-app');
    const links = destinations.map(({ path, label, icon }) => `
      <a href="${path}" aria-label="${label}" data-destination="${path}">
        <svg aria-hidden="true" viewBox="0 0 24 24"><use href="${icons}#${icon}"></use></svg>
        <span aria-hidden="true">${label}</span>
      </a>`).join('');
    const prices = ['STRN', 'MOON'].map((token) => `
      <div><dt>${token}</dt><dd aria-label="Price unavailable">—</dd></div>`).join('');
    const homeLinks = page === '/'
      ? '<a class="saturn-section-link" href="#ecosystem">Ecosystem</a><a class="saturn-section-link" href="#roadmap">Roadmap</a>'
      : '';
    const action = page === '/launchpad/'
      ? '<a class="saturn-app-action" href="/launchpad/testnet/">Testnet tools</a>'
      : '<a class="saturn-app-action" href="/launchpad/">Launchpad</a>';

    this.innerHTML = `
      <aside class="saturn-rail" aria-label="Saturn">
        <a class="saturn-logo" href="/" aria-label="Saturn home"><img src="${logo}" width="50" height="50" alt=""></a>
        <nav class="saturn-rail-links" aria-label="Saturn navigation">${links}</nav>
      </aside>
      <header class="saturn-app-bar">
        <a class="saturn-logo saturn-mobile-logo" href="/" aria-label="Saturn home"><img src="${logo}" width="38" height="38" alt=""></a>
        <dl class="saturn-prices" aria-label="Token prices" title="STRN and MOON prices are currently unavailable">${prices}</dl>
        <nav class="saturn-header-links" aria-label="Page links">${homeLinks}${action}</nav>
      </header>`;
    this.updateCurrent();
    window.addEventListener('hashchange', this.updateCurrent);
  }

  disconnectedCallback() {
    window.removeEventListener('hashchange', this.updateCurrent);
  }

  updateCurrent = () => {
    const current = page === '/' && window.location.hash === '#mint' ? '/#mint' : page;
    for (const link of this.querySelectorAll('[data-destination]')) {
      if (link.dataset.destination === current) link.setAttribute('aria-current', current.includes('#') ? 'location' : 'page');
      else link.removeAttribute('aria-current');
    }
  };
}

customElements.define('saturn-shell', SaturnShell);
