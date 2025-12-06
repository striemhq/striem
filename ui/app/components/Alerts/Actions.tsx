"use client";

interface Action {
  id: string;
  title: string;
}

interface Alert {
  id: string;
  time: string;
  severity: string;
  title: string;
}

interface ActionConfirmModalProps {
  action: Action;
  alert: Alert;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function ActionConfirmModal({ action, alert, onConfirm, onCancel }: ActionConfirmModalProps) {
  return (
    <div className="modal-overlay">
      <div className="modal-md">
        {/* Header */}
        <div className="modal-header">
          <h3 className="heading-card">Confirm Action</h3>
        </div>

        {/* Content */}
        <div className="p-6">
          <p className="text-gray-700 mb-4">
            Are you sure you want to execute the following action?
          </p>
          
          <div className="info-box">
            <div>
              <span className="text-sm font-medium text-gray-500">Action:</span>
              <p className="text-sm text-gray-900 font-medium">{action.title}</p>
            </div>
            <div>
              <span className="text-sm font-medium text-gray-500">Alert:</span>
              <p className="text-sm text-gray-900">{alert.title}</p>
            </div>
            <div>
              <span className="text-sm font-medium text-gray-500">Alert ID:</span>
              <p className="text-sm text-gray-900 font-mono">{alert.id}</p>
            </div>
          </div>

          <div className="mt-4 alert-warning">
            <div className="flex items-start">
              <span className="alert-icon-warning mr-2">⚠️</span>
              <p className="alert-text-warning">
                This action cannot be undone. Please confirm that you want to proceed.
              </p>
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="modal-footer">
          <button
            onClick={onCancel}
            className="btn-ghost"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            className="btn-primary"
          >
            Confirm
          </button>
        </div>
      </div>
    </div>
  );
}
