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
const mobile = window.matchMedia('(max-width: 640px)');

class SaturnShell extends HTMLElement {
  connectedCallback() {
    if (preview) return;
    document.body.classList.add('saturn-app');
    const links = destinations.map(({ path, label, icon }, index) => `
      <a href="${path}" data-destination="${path}" ${index < 3 ? 'data-mobile-shortcut' : ''}>
        <svg aria-hidden="true" viewBox="0 0 24 24"><use href="${icons}#${icon}"></use></svg>
        <span>${label}</span>
      </a>`).join('');
    const prices = ['STRN', 'MOON'].map((token) => `
      <div><dt>${token}</dt><dd aria-label="Price unavailable">—</dd></div>`).join('');
    const homeLinks = page === '/'
      ? '<a class="saturn-section-link" href="#ecosystem">Ecosystem</a><a class="saturn-section-link" href="#roadmap">Roadmap</a>'
      : '';
    const action = page === '/launchpad/'
      ? '<a class="saturn-app-action" href="/launchpad/testnet/">Testnet tools</a>'
      : '<a class="saturn-app-action" href="/launchpad/">Launchpad</a>';
    const wallet = page === '/launchpad/testnet/'
      ? '<span class="saturn-mobile-wallet"></span>'
      : '<a class="saturn-mobile-wallet" href="/launchpad/testnet/#wallet" aria-label="Connect wallet on Stellar testnet">Connect wallet</a>';

    this.innerHTML = `
      <aside class="saturn-rail" aria-label="Saturn">
        <a class="saturn-logo" href="/" aria-label="Saturn home"><img src="${logo}" width="50" height="50" alt=""></a>
        <nav class="saturn-rail-links" aria-label="Saturn navigation">${links}
          <button type="button" class="saturn-menu-toggle" aria-haspopup="dialog" aria-controls="saturn-menu">
            <svg aria-hidden="true" viewBox="0 0 24 24"><use href="${icons}#menu"></use></svg><span>Menu</span>
          </button></nav>
      </aside>
      <header class="saturn-app-bar">
        <a class="saturn-logo saturn-mobile-logo" href="/" aria-label="Saturn home"><img src="${logo}" width="38" height="38" alt=""></a>
        <dl class="saturn-prices" aria-label="Token prices" title="STRN and MOON prices are currently unavailable">${prices}</dl>
        <nav class="saturn-header-links" aria-label="Page links">${homeLinks}${action}</nav>
        ${wallet}
      </header>
      <dialog class="saturn-menu" id="saturn-menu" aria-labelledby="saturn-menu-title">
        <div class="saturn-menu-heading"><h2 id="saturn-menu-title">Menu</h2><form method="dialog"><button type="submit" class="saturn-menu-close" aria-label="Close menu" autofocus>×</button></form></div>
        <nav class="saturn-menu-links" aria-label="Full navigation">${links}</nav>
        <nav class="saturn-menu-sections" aria-label="Saturn information"><a href="/#ecosystem">Ecosystem</a><a href="/#roadmap">Roadmap</a></nav>
        <dl class="saturn-prices" aria-label="Token prices" title="STRN and MOON prices are currently unavailable">${prices}</dl>
      </dialog>`;
    this.menu = this.querySelector('.saturn-menu');
    this.toggle = this.querySelector('.saturn-menu-toggle');
    this.toggle.addEventListener('click', () => this.menu.showModal());
    this.menu.addEventListener('click', (event) => {
      if (event.target.closest('a') || event.target === this.menu) this.menu.close();
    });
    if (page === '/launchpad/testnet/') {
      this.walletButton = document.getElementById('connect');
      this.walletToolbar = this.walletButton.parentElement;
    }
    this.updateMobile();
    this.updateCurrent();
    mobile.addEventListener('change', this.updateMobile);
    window.addEventListener('hashchange', this.updateCurrent);
  }

  disconnectedCallback() {
    mobile.removeEventListener('change', this.updateMobile);
    window.removeEventListener('hashchange', this.updateCurrent);
  }

  updateMobile = () => {
    if (!mobile.matches && this.menu.open) this.menu.close();
    if (this.walletButton) {
      const target = mobile.matches ? this.querySelector('.saturn-mobile-wallet') : this.walletToolbar;
      if (this.walletButton.parentElement !== target) target.append(this.walletButton);
    }
  };

  updateCurrent = () => {
    const current = page === '/' && window.location.hash === '#mint' ? '/#mint' : page;
    this.toggle.classList.toggle('saturn-menu-current', !destinations.slice(0, 3).some(({ path }) => path === current));
    for (const link of this.querySelectorAll('[data-destination]')) {
      if (link.dataset.destination === current) link.setAttribute('aria-current', current.includes('#') ? 'location' : 'page');
      else link.removeAttribute('aria-current');
    }
  };
}

customElements.define('saturn-shell', SaturnShell);
