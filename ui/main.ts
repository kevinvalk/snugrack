import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';

mount(App, { target: document.getElementById('app')! });

// Feedback for the agent, in development only: a build leaves it out entirely.
if (import.meta.env.DEV) import('../feedback/Overlay.svelte').then(({ default: Overlay }) => mount(Overlay, { target: document.body }));
