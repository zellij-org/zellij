// Modal Library - modals.js

function createModalStyles() {
  // Check if styles already exist
  if (document.querySelector('#modal-styles')) return;
  
  // Component-based color variables - edit these to customize modal appearance
  
  // Security Token Modal Colors
  const securityTokenTitle = '#7E9FBE';
  const securityTokenFrame = '#7E9FBE';
  const securityTokenInputBorder = '#7E9FBE';
  const securityTokenCancelButton = '#BE616B';
  const securityTokenSubmitButton = '#A3BD8D';
  const securityTokenCheckboxAccent = '#A3BD8D';
  
  // Error Modal Colors
  const errorTitle = '#BE616B';
  const errorFrame = '#BE616B';
  const errorDismissButton = '#BE616B';
  
  // General UI Colors
  const submitButtonText = '#1a1a1a';
  
  // Light mode colors
  const lightModalBackground = 'white';
  const lightContentBackground = 'white';
  const lightInputBackground = 'white';
  const lightTextPrimary = '#333';
  const lightTextSecondary = '#666';
  
  // Dark mode colors
  const darkModalBackground = '#2a2a2a';
  const darkContentBackground = '#2a2a2a';
  const darkInputBackground = '#3a3a3a';
  const darkTextPrimary = '#e0e0e0';
  const darkTextSecondary = '#ccc';
  
  const style = document.createElement('style');
  style.id = 'modal-styles';
  style.textContent = `
    .security-modal { position:fixed;top:0;left:0;width:100%;height:100%;background:${lightModalBackground};display:flex;align-items:center;justify-content:center;z-index:9999; }
    .security-modal-content { background:${lightContentBackground};color:${lightTextPrimary};padding:20px;border-radius:8px;box-shadow:0 4px 6px rgba(0,0,0,0.3);max-width:400px;width:90%;border:2px solid ${securityTokenFrame}; }
    .security-modal h3 { margin:0 0 15px 0;color:${securityTokenTitle}; }
    .security-modal.error .security-modal-content { border-color:${errorFrame}; }
    .security-modal.error h3 { color:${errorTitle}; }
    .security-modal input[type="password"] { width:100%;padding:8px;margin-bottom:10px;border:2px solid ${securityTokenInputBorder};border-radius:4px;box-sizing:border-box;background:${lightInputBackground};color:${lightTextPrimary}; }
    .security-modal label { display:flex;align-items:center;margin-bottom:15px;cursor:pointer;color:${lightTextPrimary}; }
    .security-modal input[type="checkbox"] { margin-right:8px;accent-color:${securityTokenCheckboxAccent}; }
    .security-modal .cancel-btn { margin-right:10px;padding:8px 16px;border:2px solid ${securityTokenCancelButton};background:${lightContentBackground};color:${securityTokenCancelButton};border-radius:4px;cursor:pointer; }
    .security-modal .submit-btn { padding:8px 16px;border:none;background:${securityTokenSubmitButton};color:${submitButtonText};border-radius:4px;cursor:pointer; }
    .security-modal .dismiss-btn { padding:8px 16px;border:2px solid ${securityTokenTitle};background:${lightContentBackground};color:${securityTokenTitle};border-radius:4px;cursor:pointer; }
    .security-modal.error .dismiss-btn { border-color:${errorDismissButton};color:${errorDismissButton}; }
    .security-modal .error-description { margin:15px 0;color:${lightTextSecondary};line-height:1.4; }
    
    @media (prefers-color-scheme: dark) {
      .security-modal { background:${darkModalBackground}; }
      .security-modal-content { background:${darkContentBackground};color:${darkTextPrimary}; }
      .security-modal input[type="password"] { background:${darkInputBackground};color:${darkTextPrimary}; }
      .security-modal label { color:${darkTextPrimary}; }
      .security-modal .cancel-btn { background:${darkInputBackground}; }
      .security-modal .dismiss-btn { background:${darkInputBackground}; }
      .security-modal.error .dismiss-btn { background:${darkInputBackground}; }
      .security-modal .error-description { color:${darkTextSecondary}; }
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
        <h3>Enter Security Token</h3>
        <input type="password" id="token" placeholder="Security token">
        <label>
          <input type="checkbox" id="remember">
          Remember me
        </label>
        <div style="text-align:right">
          <button id="cancel" class="cancel-btn">Cancel</button>
          <button id="submit" class="submit-btn">Submit</button>
        </div>
      </div>
    `;
    
    document.body.appendChild(modal);
    modal.querySelector('#token').focus();
    
    // Keyboard event handler
    const handleKeydown = (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        handleSubmit();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        handleCancel();
      }
    };
    
    // Add keyboard event listener
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
        <div style="text-align:right">
          <button id="dismiss" class="dismiss-btn">Dismiss</button>
        </div>
      </div>
    `;
    
    document.body.appendChild(modal);
    modal.querySelector('#dismiss').focus();
    
    // Keyboard event handler for error modal
    const handleKeydown = (e) => {
      if (e.key === 'Enter' || e.key === 'Escape') {
        e.preventDefault();
        cleanup();
      }
    };
    
    // Add keyboard event listener
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
