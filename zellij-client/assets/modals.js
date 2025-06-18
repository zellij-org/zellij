function createModalStyles() {
  if (document.querySelector('#modal-styles')) return;
  
  const zellijGreen = '#A3BD8D';
  const zellijGreenDark = '#7A9B6A';
  const zellijBlue = '#7E9FBE';
  const zellijBlueDark = '#5A7EA0';
  const zellijYellow = '#EACB8B';
  const errorRed = '#BE616B';
  const errorRedDark = '#A04E57';
  
  const terminalDark = '#000000';
  const terminalMedium = '#1C1C1C';
  const terminalLight = '#3A3A3A';
  const terminalText = '#FFFFFF';
  const terminalTextDim = '#CCCCCC';
  
  const terminalLightBg = '#FFFFFF';
  const terminalLightMedium = '#F0F0F0';
  const terminalLightText = '#000000';
  const terminalLightTextDim = '#666666';
  
  const style = document.createElement('style');
  style.id = 'modal-styles';
  style.textContent = `
    @import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&display=swap');
    
    .security-modal {
      position: fixed;
      top: 0;
      left: 0;
      width: 100%;
      height: 100%;
      background: rgba(28, 28, 28, 0.95);
      display: flex;
      align-items: center;
      justify-content: center;
      z-index: 9999;
      font-family: 'JetBrains Mono', 'Consolas', 'Monaco', 'Courier New', monospace;
    }
    
    .security-modal-content {
      background: ${terminalDark};
      color: ${terminalText};
      padding: 24px;
      border-radius: 0;
      border: 2px solid ${zellijGreen};
      box-shadow: 0 0 20px rgba(127, 176, 105, 0.3);
      max-width: 420px;
      width: 90%;
      position: relative;
    }
    
    .security-modal-content::before {
      content: '';
      position: absolute;
      top: -2px;
      left: -2px;
      right: -2px;
      bottom: -2px;
      background: ${zellijGreen};
      border-radius: 0;
      z-index: -1;
    }
    
    .security-modal h3 {
      margin: 0 0 20px 0;
      color: ${zellijBlue};
      font-size: 16px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 1px;
      border-bottom: 1px solid ${terminalLight};
      padding-bottom: 8px;
    }
    
    .security-modal.error .security-modal-content {
      border-color: ${errorRed};
      box-shadow: 0 0 20px rgba(190, 97, 107, 0.3);
    }
    
    .security-modal.error .security-modal-content::before {
      background: ${errorRed};
    }
    
    .security-modal.error h3 {
      color: ${errorRed};
    }
    
    .security-modal input[type="password"] {
      width: 100%;
      padding: 12px 16px;
      margin-bottom: 16px;
      border: 1px solid ${terminalLight};
      border-radius: 0;
      box-sizing: border-box;
      background: ${terminalMedium};
      color: ${terminalText};
      font-family: inherit;
      font-size: 14px;
    }
    
    .security-modal input[type="password"]:focus {
      outline: none;
      border-color: ${zellijBlue};
      box-shadow: 0 0 0 1px ${zellijBlue};
      background: ${terminalLight};
    }
    
    .security-modal label {
      display: flex;
      align-items: center;
      margin-bottom: 20px;
      cursor: pointer;
      color: ${terminalTextDim};
      font-size: 13px;
      user-select: none;
    }
    
    .security-modal input[type="checkbox"] {
      appearance: none;
      width: 16px;
      height: 16px;
      border: 1px solid ${terminalLight};
      margin-right: 10px;
      background: ${terminalMedium};
      position: relative;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    
    .security-modal input[type="checkbox"]:checked {
      background: ${zellijGreen};
      border-color: ${zellijGreen};
    }
    
    .security-modal input[type="checkbox"]:checked::after {
      content: '✓';
      color: ${terminalDark};
      font-size: 12px;
      font-weight: bold;
      line-height: 1;
    }
    
    .security-modal .button-row {
      display: flex;
      gap: 12px;
      justify-content: flex-end;
      margin-top: 24px;
    }
    
    .security-modal button {
      padding: 10px 20px;
      border: 1px solid;
      border-radius: 0;
      cursor: pointer;
      font-family: inherit;
      font-size: 13px;
      font-weight: 500;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      min-width: 80px;
    }
    
    .security-modal .cancel-btn {
      background: transparent;
      color: ${terminalTextDim};
      border-color: ${terminalLight};
    }
    
    .security-modal .cancel-btn:hover {
      background: ${terminalLight};
      color: ${terminalText};
    }
    
    .security-modal .submit-btn {
      background: ${zellijGreen};
      color: ${terminalDark};
      border-color: ${zellijGreen};
    }
    
    .security-modal .submit-btn:hover {
      background: ${zellijGreenDark};
      border-color: ${zellijGreenDark};
      color: white;
    }
    
    .security-modal .dismiss-btn {
      background: transparent;
      color: ${terminalText};
      border-color: ${terminalLight};
    }
    
    .security-modal .dismiss-btn:hover {
      background: ${terminalLight};
    }
    
    .security-modal.error .dismiss-btn {
      border-color: ${errorRed};
      color: ${errorRed};
    }
    
    .security-modal.error .dismiss-btn:hover {
      background: rgba(190, 97, 107, 0.2);
    }
    
    .security-modal .error-description {
      margin: 16px 0 20px 0;
      color: ${terminalTextDim};
      line-height: 1.5;
      font-size: 14px;
      padding: 12px;
      background: rgba(190, 97, 107, 0.1);
      border-left: 3px solid ${errorRed};
    }
    
    .security-modal .status-bar {
      position: absolute;
      bottom: -2px;
      left: -2px;
      right: -2px;
      height: 3px;
      background: ${zellijGreen};
    }
    
    .security-modal.error .status-bar {
      background: ${errorRed};
    }
    
    @media (prefers-color-scheme: light) {
      .security-modal {
        background: rgba(255, 255, 255, 0.95);
      }
      
      .security-modal-content {
        background: ${terminalLightBg};
        color: ${terminalLightText};
        border-color: ${zellijBlueDark};
        box-shadow: 0 0 20px rgba(90, 126, 160, 0.3);
      }
      
      .security-modal-content::before {
        background: ${zellijBlueDark};
      }
      
      .security-modal h3 {
        color: ${zellijBlueDark};
        border-bottom-color: ${terminalLightMedium};
      }
      
      .security-modal input[type="password"] {
        background: white;
        border-color: ${zellijBlueDark};
        color: ${terminalLightText};
      }
      
      .security-modal input[type="password"]:focus {
        border-color: ${zellijBlueDark};
        box-shadow: 0 0 0 1px ${zellijBlueDark};
        background: ${terminalLightBg};
      }
      
      .security-modal label {
        color: ${terminalLightTextDim};
      }
      
      .security-modal input[type="checkbox"] {
        background: white;
        border-color: ${zellijBlueDark};
      }
      
      .security-modal input[type="checkbox"]:checked {
        background: ${zellijGreenDark};
        border-color: ${zellijGreenDark};
      }
      
      .security-modal input[type="checkbox"]:checked::after {
        color: white;
      }
      
      .security-modal .cancel-btn {
        background: ${terminalLightBg};
        color: ${terminalLightTextDim};
        border-color: ${zellijBlueDark};
      }
      
      .security-modal .cancel-btn:hover {
        background: ${terminalLightMedium};
        color: ${terminalLightText};
      }
      
      .security-modal .submit-btn {
        background: ${zellijGreenDark};
        border-color: ${zellijGreenDark};
        color: white;
      }
      
      .security-modal .submit-btn:hover {
        background: ${zellijGreen};
        border-color: ${zellijGreen};
        color: ${terminalDark};
      }
      
      .security-modal .dismiss-btn {
        background: ${terminalLightBg};
        color: ${terminalLightText};
        border-color: ${terminalLightMedium};
      }
      
      .security-modal .dismiss-btn:hover {
        background: ${terminalLightMedium};
      }
      
      .security-modal.error .dismiss-btn {
        border-color: ${errorRedDark};
        color: ${errorRedDark};
        background: ${terminalLightBg};
      }
      
      .security-modal.error .dismiss-btn:hover {
        background: rgba(160, 78, 87, 0.2);
        color: ${errorRedDark};
        border-color: ${errorRedDark};
      }
      
      .security-modal .error-description {
        color: ${terminalLightTextDim};
        background: rgba(160, 78, 87, 0.05);
      }
      
      .security-modal .status-bar {
        background: ${zellijBlueDark};
      }
    }
  `;
  document.head.appendChild(style);
}

function getSecurityToken() {
  return new Promise((resolve) => {
    createModalStyles();
    
    const modal = document.createElement('div');
    modal.className = 'security-modal';
    
    modal.innerHTML = `
      <div class="security-modal-content">
        <h3>Security Token Required</h3>
        <input type="password" id="token" placeholder="Enter your security token">
        <label>
          <input type="checkbox" id="remember">
          Remember me
        </label>
        <div class="button-row">
          <button id="cancel" class="cancel-btn">Cancel</button>
          <button id="submit" class="submit-btn">Authenticate</button>
        </div>
        <div class="status-bar"></div>
      </div>
    `;
    
    document.body.appendChild(modal);
    modal.querySelector('#token').focus();
    
    const handleKeydown = (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        handleSubmit();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        handleCancel();
      }
    };
    
    modal.addEventListener('keydown', handleKeydown);
    
    const cleanup = () => {
      modal.removeEventListener('keydown', handleKeydown);
      document.body.removeChild(modal);
    };
    
    const handleSubmit = () => {
      const token = modal.querySelector('#token').value;
      const remember = modal.querySelector('#remember').checked;
      cleanup();
      resolve({ token, remember });
    };
    
    const handleCancel = () => {
      cleanup();
      resolve(null);
    };
    
    modal.querySelector('#submit').onclick = handleSubmit;
    modal.querySelector('#cancel').onclick = handleCancel;
    
    modal.onclick = (e) => {
      if (e.target === modal) {
        handleCancel();
      }
    };
  });
}

function showErrorModal(title, description) {
  return new Promise((resolve) => {
    createModalStyles();
    
    const modal = document.createElement('div');
    modal.className = 'security-modal error';
    
    modal.innerHTML = `
      <div class="security-modal-content">
        <h3>${title}</h3>
        <div class="error-description">${description}</div>
        <div class="button-row">
          <button id="dismiss" class="dismiss-btn">Acknowledge</button>
        </div>
        <div class="status-bar"></div>
      </div>
    `;
    
    document.body.appendChild(modal);
    modal.querySelector('#dismiss').focus();
    
    const handleKeydown = (e) => {
      if (e.key === 'Enter' || e.key === 'Escape') {
        e.preventDefault();
        cleanup();
      }
    };
    
    modal.addEventListener('keydown', handleKeydown);
    
    const cleanup = () => {
      modal.removeEventListener('keydown', handleKeydown);
      document.body.removeChild(modal);
      resolve();
    };
    
    modal.querySelector('#dismiss').onclick = cleanup;
    
    modal.onclick = (e) => {
      if (e.target === modal) {
        cleanup();
      }
    };
  });
}

function showReconnectionModal(attemptNumber, delaySeconds) {
  return new Promise((resolve) => {
    createModalStyles();
    
    const modal = document.createElement('div');
    modal.className = 'security-modal';
    modal.style.background = 'rgba(28, 28, 28, 0.85)'; // More transparent to show terminal
    
    const isFirstAttempt = attemptNumber === 1;
    const title = isFirstAttempt ? 'Connection Lost' : 'Reconnection Failed';
    const message = isFirstAttempt 
      ? `Reconnecting in <span id="countdown">${delaySeconds}</span> second${delaySeconds > 1 ? 's' : ''}...`
      : `Retrying in <span id="countdown">${delaySeconds}</span> second${delaySeconds > 1 ? 's' : ''}... (Attempt ${attemptNumber})`;
    
    modal.innerHTML = `
      <div class="security-modal-content">
        <h3 id="modal-title">${title}</h3>
        <div class="error-description" id="modal-message">${message}</div>
        <div class="button-row" id="button-row">
          <button id="cancel" class="cancel-btn">Cancel</button>
          <button id="reconnect" class="submit-btn">Reconnect Now</button>
        </div>
        <div class="status-bar"></div>
      </div>
    `;
    
    document.body.appendChild(modal);
    modal.querySelector('#reconnect').focus();
    
    let countdownInterval;
    let remainingSeconds = delaySeconds;
    let isCheckingConnection = false;
    
    const updateCountdown = () => {
      const countdownElement = modal.querySelector('#countdown');
      if (countdownElement && !isCheckingConnection) {
        countdownElement.textContent = remainingSeconds;
      }
      remainingSeconds--;
      
      if (remainingSeconds < 0 && !isCheckingConnection) {
        clearInterval(countdownInterval);
        handleReconnect();
      }
    };
    
    const showConnectionCheck = () => {
      isCheckingConnection = true;
      if (countdownInterval) {
        clearInterval(countdownInterval);
      }
      
      const messageElement = modal.querySelector('#modal-message');
      messageElement.innerHTML = 'Connecting...';
    };
    
    countdownInterval = setInterval(updateCountdown, 1000);
    
    const handleKeydown = (e) => {
      if (isCheckingConnection) return;
      
      if (e.key === 'Enter') {
        e.preventDefault();
        handleReconnect();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        handleCancel();
      }
    };
    
    modal.addEventListener('keydown', handleKeydown);
    
    const cleanup = () => {
      if (countdownInterval) {
        clearInterval(countdownInterval);
      }
      modal.removeEventListener('keydown', handleKeydown);
      if (document.body.contains(modal)) {
        document.body.removeChild(modal);
      }
    };
    
    const handleReconnect = () => {
      showConnectionCheck();
      // Don't cleanup here - let the parent handle it
      resolve({ action: 'reconnect', cleanup, modal });
    };
    
    const handleCancel = () => {
      if (isCheckingConnection) return;
      cleanup();
      resolve({ action: 'cancel' });
    };
    
    modal.querySelector('#reconnect').onclick = handleReconnect;
    modal.querySelector('#cancel').onclick = handleCancel;
    
    modal.onclick = (e) => {
      if (e.target === modal && !isCheckingConnection) {
        handleCancel();
      }
    };
  });
}
