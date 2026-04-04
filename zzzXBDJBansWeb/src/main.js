import { createApp } from 'vue'
import './style.css'
import App from './App.vue'
import router from './router'
import { useAuthStore } from './composables/useAuthStore'

const getDefaultApiBaseUrl = () => {
    if (import.meta.env.PROD) {
        return '/api'
    }

    return import.meta.env.VITE_API_BASE_URL || '/api'
}

const loadConfig = async () => {
    try {
        const response = await fetch('/config.json');
        const config = await response.json();
        window.runtimeConfig = config;
    } catch (error) {
        console.error('Failed to load config, using defaults', error);
        window.runtimeConfig = {
            apiBaseUrl: getDefaultApiBaseUrl()
        };
    }
};

loadConfig().then(async () => {
    const authStore = useAuthStore();
    await authStore.checkAuth();

    createApp(App)
        .use(router)
        .mount('#app')
});
