// Silo Panel i18n — dependency-free label dictionary + t() helper.
// Default English; en + tr are complete, other locales cover the visible
// chrome and fall back to English per key. Persisted in localStorage.
'use strict';

const SILO_LOCALES = [
    ['en', 'English'], ['tr', 'Türkçe'], ['it', 'Italiano'], ['fr', 'Français'],
    ['de', 'Deutsch'], ['zh', '中文'], ['ja', '日本語'], ['hi', 'हिन्दी'], ['ar', 'العربية'],
];

const SILO_I18N = {
    en: {
        login_hint: 'Operator access — enter the admin token (env ADMIN_TOKEN, or the one printed in the server log).',
        authorize: 'AUTHORIZE', lock: 'LOCK', denied: 'ACCESS DENIED — invalid token',
        nav_overview: 'OVERVIEW', nav_connections: 'CONNECTIONS', nav_rooms: 'ROOMS', nav_logs: 'LOGS',
        nav_webhooks: 'WEBHOOKS', nav_plugins: 'PLUGINS', nav_settings: 'SETTINGS',
        kpi_conns: 'ACTIVE CONNECTIONS', kpi_rooms: 'ROOMS', kpi_uptime: 'UPTIME', kpi_rx: 'RX RATE',
        kpi_tx: 'TX RATE', kpi_dropped: 'DROPPED MSGS', kpi_plugins: 'PLUGINS', kpi_webhooks: 'WEBHOOKS',
        throughput: 'THROUGHPUT — LAST 60s', lb_hints: 'LOAD BALANCER HINTS', live_conns: 'LIVE CONNECTIONS',
        rooms_title: 'ROOMS', live_logs: 'LIVE LOG STREAM', register_endpoint: 'REGISTER ENDPOINT',
        endpoints: 'ENDPOINTS', delivery_audit: 'DELIVERY AUDIT', create: 'CREATE',
        room_ops: 'ROOM / NODE OPERATIONS', engage_drain: 'ENGAGE DRAIN', release_drain: 'RELEASE DRAIN',
        reload_users: 'RELOAD USER FILE', config_title: 'NON-SECRET CONFIGURATION',
        drain_note: 'Drain: /readyz turns 503 and new connections are refused; existing sessions stay up.',
        kick: 'KICK', test: 'TEST', pause: 'PAUSE', resume: 'RESUME', del: 'DEL',
        enable: 'ENABLE', disable: 'DISABLE', reload: 'RELOAD',
        th_user: 'USER', th_role: 'ROLE', th_room: 'ROOM', th_remote: 'REMOTE', th_connected: 'CONNECTED',
        th_name: 'NAME', th_members: 'MEMBERS', th_url: 'URL', th_events: 'EVENTS',
        th_time: 'TIME', th_endpoint: 'ENDPOINT', th_event: 'EVENT', th_outcome: 'OUTCOME', th_try: 'TRY',
    },
    tr: {
        login_hint: 'Operatör erişimi — admin token girin (env ADMIN_TOKEN veya sunucu günlüğünde yazan).',
        authorize: 'YETKİLENDİR', lock: 'KİLİTLE', denied: 'ERİŞİM REDDEDİLDİ — geçersiz token',
        nav_overview: 'GENEL BAKIŞ', nav_connections: 'BAĞLANTILAR', nav_rooms: 'ODALAR', nav_logs: 'GÜNLÜKLER',
        nav_webhooks: 'WEBHOOK\'LAR', nav_plugins: 'EKLENTİLER', nav_settings: 'AYARLAR',
        kpi_conns: 'AKTİF BAĞLANTILAR', kpi_rooms: 'ODALAR', kpi_uptime: 'ÇALIŞMA SÜRESİ', kpi_rx: 'RX HIZI',
        kpi_tx: 'TX HIZI', kpi_dropped: 'DÜŞEN MESAJLAR', kpi_plugins: 'EKLENTİLER', kpi_webhooks: 'WEBHOOK\'LAR',
        throughput: 'AKTARIM — SON 60 sn', lb_hints: 'YÜK DENGELEYİCİ İPUÇLARI', live_conns: 'CANLI BAĞLANTILAR',
        rooms_title: 'ODALAR', live_logs: 'CANLI GÜNLÜK AKIŞI', register_endpoint: 'UÇ NOKTA KAYDET',
        endpoints: 'UÇ NOKTALAR', delivery_audit: 'TESLİMAT DENETİMİ', create: 'OLUŞTUR',
        room_ops: 'ODA / DÜĞÜM İŞLEMLERİ', engage_drain: 'DRAIN BAŞLAT', release_drain: 'DRAIN BIRAK',
        reload_users: 'KULLANICI DOSYASINI YENİLE', config_title: 'GİZLİ OLMAYAN YAPILANDIRMA',
        drain_note: 'Drain: /readyz 503 döner ve yeni bağlantılar reddedilir; mevcut oturumlar açık kalır.',
        kick: 'AT', test: 'TEST', pause: 'DURAKLAT', resume: 'SÜRDÜR', del: 'SİL',
        enable: 'ETKİNLEŞTİR', disable: 'DEVRE DIŞI', reload: 'YENİLE',
        th_user: 'KULLANICI', th_role: 'ROL', th_room: 'ODA', th_remote: 'UZAK', th_connected: 'BAĞLANDI',
        th_name: 'AD', th_members: 'ÜYELER', th_url: 'URL', th_events: 'OLAYLAR',
        th_time: 'ZAMAN', th_endpoint: 'UÇ NOKTA', th_event: 'OLAY', th_outcome: 'SONUÇ', th_try: 'DENEME',
    },
    it: {
        nav_overview: 'PANORAMICA', nav_connections: 'CONNESSIONI', nav_rooms: 'STANZE', nav_logs: 'LOG',
        nav_webhooks: 'WEBHOOK', nav_plugins: 'PLUGIN', nav_settings: 'IMPOSTAZIONI',
        authorize: 'AUTORIZZA', lock: 'BLOCCA', create: 'CREA',
    },
    fr: {
        nav_overview: 'APERÇU', nav_connections: 'CONNEXIONS', nav_rooms: 'SALONS', nav_logs: 'JOURNAUX',
        nav_webhooks: 'WEBHOOKS', nav_plugins: 'PLUGINS', nav_settings: 'RÉGLAGES',
        authorize: 'AUTORISER', lock: 'VERROUILLER', create: 'CRÉER',
    },
    de: {
        nav_overview: 'ÜBERSICHT', nav_connections: 'VERBINDUNGEN', nav_rooms: 'RÄUME', nav_logs: 'LOGS',
        nav_webhooks: 'WEBHOOKS', nav_plugins: 'PLUGINS', nav_settings: 'EINSTELLUNGEN',
        authorize: 'AUTORISIEREN', lock: 'SPERREN', create: 'ERSTELLEN',
    },
    zh: {
        nav_overview: '概览', nav_connections: '连接', nav_rooms: '房间', nav_logs: '日志',
        nav_webhooks: 'Webhook', nav_plugins: '插件', nav_settings: '设置',
        authorize: '授权', lock: '锁定', create: '创建',
    },
    ja: {
        nav_overview: '概要', nav_connections: '接続', nav_rooms: 'ルーム', nav_logs: 'ログ',
        nav_webhooks: 'Webhook', nav_plugins: 'プラグイン', nav_settings: '設定',
        authorize: '認証', lock: 'ロック', create: '作成',
    },
    hi: {
        nav_overview: 'अवलोकन', nav_connections: 'कनेक्शन', nav_rooms: 'रूम', nav_logs: 'लॉग',
        nav_webhooks: 'वेबहुक', nav_plugins: 'प्लगइन', nav_settings: 'सेटिंग्स',
        authorize: 'अधिकृत करें', lock: 'लॉक', create: 'बनाएँ',
    },
    ar: {
        nav_overview: 'نظرة عامة', nav_connections: 'الاتصالات', nav_rooms: 'الغرف', nav_logs: 'السجلات',
        nav_webhooks: 'Webhooks', nav_plugins: 'الإضافات', nav_settings: 'الإعدادات',
        authorize: 'تفويض', lock: 'قفل', create: 'إنشاء',
    },
};

let SILO_LANG = localStorage.getItem('adatp_silo_lang') || 'en';

function t(key) {
    return (SILO_I18N[SILO_LANG] && SILO_I18N[SILO_LANG][key]) || SILO_I18N.en[key] || key;
}

function siloSetLang(lang) {
    if (!SILO_I18N[lang]) lang = 'en';
    SILO_LANG = lang;
    localStorage.setItem('adatp_silo_lang', lang);
    document.documentElement.lang = lang;
    document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
    siloApplyI18n();
}

/** Re-labels every element carrying data-i18n="key". */
function siloApplyI18n() {
    document.querySelectorAll('[data-i18n]').forEach((el) => {
        el.textContent = t(el.dataset.i18n);
    });
    const sel = document.getElementById('langSelect');
    if (sel && sel.value !== SILO_LANG) sel.value = SILO_LANG;
}

/** Builds the language <select> in the header. */
function siloInitLang() {
    const sel = document.getElementById('langSelect');
    if (!sel) return;
    sel.innerHTML = SILO_LOCALES.map(([code, label]) => `<option value="${code}">${label}</option>`).join('');
    sel.value = SILO_LANG;
    sel.onchange = () => siloSetLang(sel.value);
    siloSetLang(SILO_LANG);
}
