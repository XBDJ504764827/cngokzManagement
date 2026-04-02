import { createRouter, createWebHistory } from 'vue-router'
import LoginView from '../views/LoginView.vue'
import DashboardLayout from '../layouts/DashboardLayout.vue'
import { useAuthStore } from '../composables/useAuthStore'

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/',
            name: 'login',
            component: LoginView,
        },
        {
            path: '/apply',
            name: 'apply',
            component: () => import('../views/WhitelistApply.vue'),
        },
        {
            path: '/whitelist-status',
            name: 'whitelist-status',
            component: () => import('../views/WhitelistStatus.vue'),
        },
        {
            path: '/bans',
            name: 'public-bans',
            component: () => import('../views/PublicBanList.vue'),
        },
        {
            path: '/admin',
            component: DashboardLayout,
            meta: { requiresAuth: true },
            children: [
                {
                    path: '',
                    redirect: '/admin/community'
                },
                {
                    path: 'community',
                    name: 'community',
                    component: () => import('../views/admin/CommunityManagement.vue')
                },
                {
                    path: 'bans',
                    name: 'bans',
                    component: () => import('../views/admin/BanList.vue')
                },
                {
                    path: 'admins',
                    name: 'admins',
                    component: () => import('../views/admin/AdminList.vue')
                },
                {
                    path: 'interrupt-pause',
                    name: 'interrupt-pause',
                    component: () => import('../views/admin/InterruptPauseManagement.vue')
                },
                {
                    path: 'logs',
                    name: 'logs',
                    component: () => import('../views/admin/AuditLog.vue'),
                    meta: { requiresSuperAdmin: true }
                },
                {
                    path: 'whitelist',
                    name: 'whitelist',
                    component: () => import('../views/admin/WhitelistManagement.vue')
                },
                {
                    path: 'verifications',
                    name: 'verifications',
                    component: () => import('../views/admin/VerificationList.vue'),
                    meta: { requiresSuperAdmin: true }
                },
            ]
        },
        {
            path: '/:pathMatch(.*)*',
            redirect: '/'
        }
    ],
})

router.beforeEach(async (to) => {
    const authStore = useAuthStore()
    const token = localStorage.getItem('token')
    const requiresAuth = to.matched.some(record => record.meta.requiresAuth)
    const requiresSuperAdmin = to.matched.some(record => record.meta.requiresSuperAdmin)
    let isAuthenticated = Boolean(authStore.currentUser.value)

    if (token && !isAuthenticated) {
        isAuthenticated = await authStore.checkAuth()
    }

    if (requiresAuth && !isAuthenticated) {
        return '/'
    }

    if (requiresSuperAdmin && authStore.currentUser.value?.role !== 'super_admin') {
        return '/admin/community'
    }

    if (to.path === '/' && isAuthenticated) {
        return '/admin'
    }

    return true
})

export default router
