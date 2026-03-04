// Stealth JavaScript for Anti-Bot Detection Bypass
// This code is injected into every page to hide automation and randomize fingerprints
// Based on: Firecrawl stealth proxy analysis + puppeteer-extra-plugin-stealth
//
// Techniques implemented:
// 1. Remove navigator.webdriver
// 2. Add window.chrome object
// 3. Spoof Permissions API
// 4. Randomize plugins
// 5. Override languages
// 6. Fix hairline detector
// 7. Canvas fingerprint randomization
// 8. WebGL vendor/renderer spoofing
// 9. Media devices enumeration
// 10. Battery API spoofing

(function() {
    'use strict';

    // ============================================================================
    // 1. Remove navigator.webdriver (most common detection)
    // ============================================================================
    
    // Delete from prototype first
    if (Object.getPrototypeOf(navigator).hasOwnProperty('webdriver')) {
        delete Object.getPrototypeOf(navigator).webdriver;
    }

    // Then redefine as undefined
    Object.defineProperty(navigator, 'webdriver', {
        get: () => undefined,
        configurable: true
    });

    // ============================================================================
    // 2. Add window.chrome object (avoid headless detection)
    // ============================================================================
    
    if (!window.chrome) {
        Object.defineProperty(window, 'chrome', {
            get: () => ({
                runtime: {},
                loadTimes: function() {},
                csi: function() {},
                app: {}
            }),
            configurable: true
        });
    }

    // ============================================================================
    // 3. Spoof Permissions API (notifications permission)
    // ============================================================================
    
    if (window.navigator && window.navigator.permissions) {
        const originalQuery = window.navigator.permissions.query;
        window.navigator.permissions.query = (parameters) => (
            parameters.name === 'notifications' ?
                Promise.resolve({ state: Notification.permission }) :
                originalQuery(parameters)
        );
    }

    // ============================================================================
    // 4. Randomize plugins (empty plugins array = headless)
    // ============================================================================
    
    Object.defineProperty(navigator, 'plugins', {
        get: () => {
            // Return realistic plugin list
            return [
                {
                    name: 'Chrome PDF Plugin',
                    filename: 'internal-pdf-viewer',
                    description: 'Portable Document Format'
                },
                {
                    name: 'Chrome PDF Viewer',
                    filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai',
                    description: 'Portable Document Format'
                },
                {
                    name: 'Native Client',
                    filename: 'internal-nacl-plugin',
                    description: 'Native Client Executable'
                }
            ];
        },
        configurable: true
    });

    // ============================================================================
    // 5. Override languages
    // ============================================================================
    
    Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en'],
        configurable: true
    });

    // ============================================================================
    // 6. Fix hairline detector (outerWidth/outerHeight)
    // ============================================================================
    
    // In headless browsers, outerWidth === innerWidth (no browser UI)
    // Real browsers have UI, so outerWidth === innerWidth and outerHeight > innerHeight
    Object.defineProperty(window, 'outerWidth', {
        get: () => window.innerWidth,
        configurable: true
    });

    Object.defineProperty(window, 'outerHeight', {
        get: () => window.innerHeight + 74,  // Chrome UI is ~74px
        configurable: true
    });

    // ============================================================================
    // 7. Canvas fingerprint randomization
    // ============================================================================
    
    // Add minimal noise to canvas to prevent fingerprinting
    const originalToDataURL = HTMLCanvasElement.prototype.toDataURL;
    const originalToBlob = HTMLCanvasElement.prototype.toBlob;
    const originalGetImageData = CanvasRenderingContext2D.prototype.getImageData;

    // Helper: Add noise to image data
    const addCanvasNoise = function(imageData) {
        // Add minimal random noise (±1-5 per channel)
        const noise = () => Math.floor(Math.random() * 10) - 5;
        
        // Only modify a few pixels to avoid visual detection
        const pixelsToModify = Math.min(10, imageData.data.length / 4);
        
        for (let i = 0; i < pixelsToModify; i++) {
            const randomIndex = Math.floor(Math.random() * (imageData.data.length / 4)) * 4;
            
            // Modify RGB (not alpha)
            imageData.data[randomIndex] = Math.min(255, Math.max(0, imageData.data[randomIndex] + noise()));
            imageData.data[randomIndex + 1] = Math.min(255, Math.max(0, imageData.data[randomIndex + 1] + noise()));
            imageData.data[randomIndex + 2] = Math.min(255, Math.max(0, imageData.data[randomIndex + 2] + noise()));
        }
        
        return imageData;
    };

    // Override toDataURL
    HTMLCanvasElement.prototype.toDataURL = function() {
        try {
            const context = this.getContext('2d');
            if (context && this.width > 0 && this.height > 0) {
                const imageData = context.getImageData(0, 0, this.width, this.height);
                const noisyData = addCanvasNoise(imageData);
                context.putImageData(noisyData, 0, 0);
            }
        } catch (e) {
            // Silently fail if canvas is tainted
        }
        
        return originalToDataURL.apply(this, arguments);
    };

    // Override getImageData
    CanvasRenderingContext2D.prototype.getImageData = function() {
        const imageData = originalGetImageData.apply(this, arguments);
        return addCanvasNoise(imageData);
    };

    // ============================================================================
    // 8. WebGL vendor/renderer spoofing
    // ============================================================================
    
    // Headless browsers often expose different WebGL info
    const getParameter = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function(parameter) {
        // 37445 = UNMASKED_VENDOR_WEBGL
        if (parameter === 37445) {
            return 'Intel Inc.';
        }
        
        // 37446 = UNMASKED_RENDERER_WEBGL
        if (parameter === 37446) {
            return 'Intel Iris OpenGL Engine';
        }
        
        return getParameter.apply(this, arguments);
    };

    // Also override for WebGL2
    if (typeof WebGL2RenderingContext !== 'undefined') {
        const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
        WebGL2RenderingContext.prototype.getParameter = function(parameter) {
            if (parameter === 37445) {
                return 'Intel Inc.';
            }
            if (parameter === 37446) {
                return 'Intel Iris OpenGL Engine';
            }
            return getParameter2.apply(this, arguments);
        };
    }

    // ============================================================================
    // 9. Media devices enumeration
    // ============================================================================
    
    // Headless browsers often have no media devices
    if (navigator.mediaDevices && navigator.mediaDevices.enumerateDevices) {
        navigator.mediaDevices.enumerateDevices = () => Promise.resolve([
            {
                deviceId: 'default',
                kind: 'audioinput',
                label: 'Default - Microphone',
                groupId: 'default-audio-group'
            },
            {
                deviceId: 'default',
                kind: 'audiooutput',
                label: 'Default - Speaker',
                groupId: 'default-audio-group'
            },
            {
                deviceId: 'default',
                kind: 'videoinput',
                label: 'Default - Camera',
                groupId: 'default-video-group'
            }
        ]);
    }

    // ============================================================================
    // 10. Battery API spoofing
    // ============================================================================
    
    // Headless browsers often have no battery or unrealistic values
    if (navigator.getBattery) {
        const originalGetBattery = navigator.getBattery;
        navigator.getBattery = () => Promise.resolve({
            charging: true,
            chargingTime: 0,
            dischargingTime: Infinity,
            level: 1.0,
            addEventListener: function() {},
            removeEventListener: function() {},
            dispatchEvent: function() { return true; }
        });
    }

    // ============================================================================
    // Additional Stealth: Conceal CDP Runtime
    // ============================================================================
    
    // Remove CDP runtime detection (Chromium DevTools Protocol)
    if (window.cdc_adoQpoasnfa76pfcZLmcfl_Array || 
        window.cdc_adoQpoasnfa76pfcZLmcfl_Promise ||
        window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol) {
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Array;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Promise;
        delete window.cdc_adoQpoasnfa76pfcZLmcfl_Symbol;
    }

    // ============================================================================
    // Stealth Injection Complete
    // ============================================================================
    
    // Optional: Log success (for debugging)
    // console.log('[Stealth] Anti-detection techniques applied successfully');

})();
