import { mount } from 'svelte';
import './app.css';
import './nc06.css';

const desktop = new URLSearchParams(window.location.search).get('desktop') === '1';
const target = document.getElementById('app')!;
if (desktop) {
  void import('./Desktop.svelte').then(({ default: Desktop }) => mount(Desktop, { target }));
} else {
  void import('./App.svelte').then(({ default: App }) => mount(App, { target }));
}
