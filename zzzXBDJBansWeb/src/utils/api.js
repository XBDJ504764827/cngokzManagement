import axios from 'axios'

const api = axios.create({
    // baseURL is set dynamically in the request interceptor
    headers: {
        'Content-Type': 'application/json'
    }
})

// Request interceptor to add token and set dynamic baseURL
api.interceptors.request.use(config => {
    // Dynamically set baseURL if it's an API request (starts with /api or relative path)
    // and if we have a runtime config loaded
    if (window.runtimeConfig && window.runtimeConfig.apiBaseUrl) {
        config.baseURL = window.runtimeConfig.apiBaseUrl;
    } else if (import.meta.env.VITE_API_BASE_URL) {
        config.baseURL = import.meta.env.VITE_API_BASE_URL;
    } else {
        config.baseURL = '/api';
    }

    const token = localStorage.getItem('token')
    if (token) {
        config.headers.Authorization = `Bearer ${token}`
    }
    return config
})

// Response interceptor to handle auth errors
api.interceptors.response.use(response => {
    return response
}, error => {
    if (error.response && error.response.status === 401) {
        // Token expired or invalid
        localStorage.removeItem('token')
        localStorage.removeItem('user')
        window.location.href = '/' // Redirect to login
    }
    return Promise.reject(error)
})

export default api
