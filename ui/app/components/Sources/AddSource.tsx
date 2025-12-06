"use client";

import { useState } from "react";

interface SourceType {
  id: string;
  name: string;
  description: string;
}

interface AddSourceModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSourceAdded: () => void;
}

interface AWSCloudTrailConfig {
  auth?: {
    aws_access_key_id?: string;
    aws_secret_access_key?: string;
    assume_role?: string;
  };
  sqs: {
    queue_url: string;
    delete_message: boolean;
    poll_secs: number;
    visibility_timeout_secs: number;
    receive_message_wait_time_secs: number;
  };
  region: string;
  decoding: {
    codec: string;
  };
}

interface OktaConfig {
  domain: string;
  token: string;
  type?: string;
}

const sourceTypes: SourceType[] = [
  {
    id: "aws_cloudtrail",
    name: "AWS CloudTrail",
    description: "Ingest AWS CloudTrail logs from SQS"
  },
  {
    id: "okta",
    name: "Okta",
    description: "Ingest logs from Okta system logs API"
  }
];

export default function AddSource({ isOpen, onClose, onSourceAdded }: AddSourceModalProps) {
  const [step, setStep] = useState<'select' | 'configure'>('select');
  const [selectedSourceType, setSelectedSourceType] = useState<string>('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // AWS CloudTrail config state
  const [awsConfig, setAwsConfig] = useState<AWSCloudTrailConfig>({
    sqs: {
      queue_url: '',
      delete_message: true,
      poll_secs: 15,
      visibility_timeout_secs: 300,
      receive_message_wait_time_secs: 0
    },
    region: 'us-east-1',
    decoding: {
      codec: 'json'
    }
  });

  // Okta config state
  const [oktaConfig, setOktaConfig] = useState<OktaConfig>({
    domain: '',
    token: '',
    type: 'okta_system_log'
  });

  const resetModal = () => {
    setStep('select');
    setSelectedSourceType('');
    setError(null);
    setLoading(false);
    setAwsConfig({
      sqs: {
        queue_url: '',
        delete_message: true,
        poll_secs: 15,
        visibility_timeout_secs: 300,
        receive_message_wait_time_secs: 0
      },
      region: 'us-east-1',
      decoding: {
        codec: 'json'
      }
    });
    setOktaConfig({
      domain: '',
      token: '',
      type: 'okta_system_log'
    });
  };

  const handleClose = () => {
    resetModal();
    onClose();
  };

  const handleNext = () => {
    if (!selectedSourceType) {
      setError('Please select a source type');
      return;
    }
    setError(null);
    setStep('configure');
  };

  const handleBack = () => {
    setStep('select');
    setError(null);
  };

  const handleSubmit = async () => {
    try {
      setLoading(true);
      setError(null);

      let config: any;
      let endpoint: string;

      if (selectedSourceType === 'aws_cloudtrail') {
        if (!awsConfig.sqs.queue_url) {
          throw new Error('Queue URL is required');
        }
        config = awsConfig;
        endpoint = `${process.env.NEXT_PUBLIC_API_URL}/sources/aws_cloudtrail`;
      } else if (selectedSourceType === 'okta') {
        if (!oktaConfig.domain || !oktaConfig.token) {
          throw new Error('Domain and token are required');
        }
        config = oktaConfig;
        endpoint = `${process.env.NEXT_PUBLIC_API_URL}/sources/okta`;
      } else {
        throw new Error('Invalid source type');
      }

      const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(config),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to create source: ${response.status} ${response.statusText} - ${errorText}`);
      }

      onSourceAdded();
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="modal-overlay">
      <div className="bg-white rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[90vh] overflow-hidden">
        <div className="p-6">
          <div className="flex justify-between items-center mb-6">
            <h2 className="text-2xl font-semibold text-gray-900">
              {step === 'select' ? 'Add New Source' : `Configure ${sourceTypes.find(s => s.id === selectedSourceType)?.name}`}
            </h2>
            <button
              onClick={handleClose}
              className="text-gray-400 hover:text-gray-600 text-2xl"
            >
              X
            </button>
          </div>

          {error && (
            <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md text-red-700">
              {error}
            </div>
          )}

          {step === 'select' && (
            <div>
              <p className="text-gray-600 mb-6">Select the type of source you want to add:</p>
              <div className="space-y-3">
                {sourceTypes.map((sourceType) => (
                  <label
                    key={sourceType.id}
                    className="flex items-start p-4 border border-gray-200 rounded-lg cursor-pointer hover:bg-gray-50"
                  >
                    <input
                      type="radio"
                      name="sourceType"
                      value={sourceType.id}
                      checked={selectedSourceType === sourceType.id}
                      onChange={(e) => setSelectedSourceType(e.target.value)}
                      className="input-radio"
                    />
                    <div className="ml-3">
                      <div className="font-medium text-gray-900">{sourceType.name}</div>
                      <div className="text-sm text-gray-500">{sourceType.description}</div>
                    </div>
                  </label>
                ))}
              </div>
            </div>
          )}

          {step === 'configure' && selectedSourceType === 'aws_cloudtrail' && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-medium text-gray-900 mb-4">SQS Configuration</h3>
                <div className="space-y-4">
                  <div>
                    <label className="input-label">
                      Queue URL *
                    </label>
                    <input
                      type="url"
                      value={awsConfig.sqs.queue_url}
                      onChange={(e) => setAwsConfig({
                        ...awsConfig,
                        sqs: { ...awsConfig.sqs, queue_url: e.target.value }
                      })}
                      className="input-base"
                      placeholder="https://sqs.us-east-1.amazonaws.com/123456789012/my-queue"
                      required
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="input-label">
                        Poll Seconds
                      </label>
                      <input
                        type="number"
                        value={awsConfig.sqs.poll_secs}
                        onChange={(e) => setAwsConfig({
                          ...awsConfig,
                          sqs: { ...awsConfig.sqs, poll_secs: parseInt(e.target.value) }
                        })}
                        className="input-base"
                        min="1"
                      />
                    </div>
                    <div>
                      <label className="input-label">
                        Visibility Timeout (seconds)
                      </label>
                      <input
                        type="number"
                        value={awsConfig.sqs.visibility_timeout_secs}
                        onChange={(e) => setAwsConfig({
                          ...awsConfig,
                          sqs: { ...awsConfig.sqs, visibility_timeout_secs: parseInt(e.target.value) }
                        })}
                        className="input-base"
                        min="0"
                      />
                    </div>
                  </div>
                  <div>
                    <label className="flex items-center">
                      <input
                        type="checkbox"
                        checked={awsConfig.sqs.delete_message}
                        onChange={(e) => setAwsConfig({
                          ...awsConfig,
                          sqs: { ...awsConfig.sqs, delete_message: e.target.checked }
                        })}
                        className="input-checkbox"
                      />
                      <span className="form-label-inline">Delete message after processing</span>
                    </label>
                  </div>
                </div>
              </div>

              <div>
                <h3 className="text-lg font-medium text-gray-900 mb-4">AWS Configuration</h3>
                <div className="space-y-4">
                  <div>
                    <label className="input-label">
                      Region *
                    </label>
                    <select
                      value={awsConfig.region}
                      onChange={(e) => setAwsConfig({ ...awsConfig, region: e.target.value })}
                      className="input-select"
                    >
                      <option value="us-east-1">us-east-1</option>
                      <option value="us-east-2">us-east-2</option>
                      <option value="us-west-1">us-west-1</option>
                      <option value="us-west-2">us-west-2</option>
                      <option value="eu-west-1">eu-west-1</option>
                      <option value="eu-central-1">eu-central-1</option>
                      <option value="ap-southeast-1">ap-southeast-1</option>
                    </select>
                  </div>
                  <div>
                    <label className="input-label">
                      AWS Access Key ID (Optional - leave empty to use IAM roles)
                    </label>
                    <input
                      type="text"
                      value={awsConfig.auth?.aws_access_key_id || ''}
                      onChange={(e) => setAwsConfig({
                        ...awsConfig,
                        auth: { ...awsConfig.auth, aws_access_key_id: e.target.value }
                      })}
                      className="input-base"
                    />
                  </div>
                  <div>
                    <label className="input-label">
                      AWS Secret Access Key (Optional)
                    </label>
                    <input
                      type="password"
                      value={awsConfig.auth?.aws_secret_access_key || ''}
                      onChange={(e) => setAwsConfig({
                        ...awsConfig,
                        auth: { ...awsConfig.auth, aws_secret_access_key: e.target.value }
                      })}
                      className="input-base"
                    />
                  </div>
                </div>
              </div>
            </div>
          )}

          {step === 'configure' && selectedSourceType === 'okta' && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-medium text-gray-900 mb-4">Okta Configuration</h3>
                <div className="space-y-4">
                  <div>
                    <label className="input-label">
                      Okta Domain *
                    </label>
                    <input
                      type="text"
                      value={oktaConfig.domain}
                      onChange={(e) => setOktaConfig({ ...oktaConfig, domain: e.target.value })}
                      className="input-base"
                      placeholder="your-domain.okta.com"
                      required
                    />
                  </div>
                  <div>
                    <label className="input-label">
                      API Token *
                    </label>
                    <input
                      type="password"
                      value={oktaConfig.token}
                      onChange={(e) => setOktaConfig({ ...oktaConfig, token: e.target.value })}
                      className="input-base"
                      placeholder="Your Okta API token"
                      required
                    />
                  </div>
                </div>
              </div>
            </div>
          )}

          <div className="flex justify-end space-x-3 pt-6 mt-6 border-t border-gray-200">
            {step === 'select' ? (
              <>
                <button
                  onClick={handleClose}
                  className="btn-tertiary"
                >
                  Cancel
                </button>
                <button
                  onClick={handleNext}
                  className="btn-primary"
                  disabled={!selectedSourceType}
                >
                  Next
                </button>
              </>
            ) : (
              <>
                <button
                  onClick={handleBack}
                  className="btn-tertiary"
                  disabled={loading}
                >
                  Back
                </button>
                <button
                  onClick={handleClose}
                  className="btn-tertiary"
                  disabled={loading}
                >
                  Cancel
                </button>
                <button
                  onClick={handleSubmit}
                  className="btn-primary"
                  disabled={loading}
                >
                  {loading ? 'Adding...' : 'Add Source'}
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
