pipeline {
    agent any

    environment {
        // Automatically mapped to ghcr.io based on docker-compose.yaml configuration
        DOCKER_REGISTRY = 'ghcr.io/femrek' 
        
        // env.TAG_NAME is automatically populated by Jenkins when a tag is built
        IMAGE_TAG = "${env.TAG_NAME}" 
        
        // Enable Docker BuildKit for advanced Dockerfile features like --mount
        DOCKER_BUILDKIT = '1'
    }

    stages {
        stage('Verify Tag') {
            steps {
                script {
                    if (!env.TAG_NAME) {
                        error('This build is not a tag. Aborting pipeline.')
                    }
                    echo "Building Docker images for tag: ${env.TAG_NAME}"
                }
            }
        }

        stage('Build Docker Images') {
            when {
                buildingTag() // Ensures this stage only runs for tags
            }
            steps {
                script {
                    // Build logger_gateway from its context and Dockerfile
                    echo "Building logger_gateway image..."
                    gatewayImage = docker.build("${DOCKER_REGISTRY}/logger_gateway:${IMAGE_TAG}", "-f logger_gateway/docker/Dockerfile logger_gateway")
                    
                    // Build validation_worker from its context and Dockerfile
                    echo "Building validation_worker image..."
                    workerImage = docker.build("${DOCKER_REGISTRY}/validation_worker:${IMAGE_TAG}", "-f validation_worker/Dockerfile validation_worker")
                }
            }
        }

        stage('Push Docker Images') {
            when {
                buildingTag()
            }
            steps {
                script {
                    // Provide the Jenkins credential ID 'github-credentials' matching your GHCR config
                    docker.withRegistry('https://ghcr.io', 'github-general-pat') {
                        gatewayImage.push()
                        gatewayImage.push('latest')
                        
                        workerImage.push()
                        workerImage.push('latest') 
                    }
                }
            }
        }

        stage('Trigger Deployment') {
            when { buildingTag() }
            steps {
                // 1. Fetch the target from Vault KV (This plugin step works perfectly for KV!)
                withVault(vaultSecrets: [[
                    engineVersion: 2,
                    path: 'secret/jenkins/deploy_config', 
                    secretValues: [[envVar: 'DEPLOY_TARGET', vaultKey: 'target']]
                ]]) {
                    // 2. Fetch the raw AppRole credentials to authenticate our manual POST request
                    withCredentials([usernamePassword(credentialsId: 'vault-approle-raw', passwordVariable: 'SECRET_ID', usernameVariable: 'ROLE_ID')]) {
                        script {
                            echo "Authenticating to Vault and minting ephemeral SSH keys via REST API..."
                            
                            // 3. Use curl and jq to handle the POST requests natively
                            sh '''
                                # Login to Vault via AppRole and extract the token
                                VAULT_TOKEN=$(curl -s --request POST --data '{"role_id":"'"${ROLE_ID}"'","secret_id":"'"${SECRET_ID}"'"}' https://vault.femrek.dev/v1/auth/approle/login | jq -r .auth.client_token)
                                
                                # Request the ephemeral SSH credentials (POST)
                                curl -s --header "X-Vault-Token: ${VAULT_TOKEN}" --header "Content-Type: application/json" --request POST --data "{}" https://vault.femrek.dev/v1/ssh/issue/jenkins-deploy > ephemeral_keys.json
                                
                                if ! jq -e '.data.private_key' ephemeral_keys.json >/dev/null; then
                                    echo "Error: Vault failed to return SSH keys. Response:"
                                    cat ephemeral_keys.json
                                    exit 1
                                fi
                                
                                # Extract and format the keys
                                jq -r .data.private_key ephemeral_keys.json > vault_ssh_key
                                jq -r .data.signed_key ephemeral_keys.json > vault_ssh_key-cert.pub
                                
                                # Lock down the private key
                                chmod 600 vault_ssh_key
                                
                                echo "Deploying to production target via Vault-signed SSH..."
                                
                                # Execute the deployment
                                cp docker/deploy.sh.template docker/deploy.sh
                                ssh -i vault_ssh_key -o StrictHostKeyChecking=no ${DEPLOY_TARGET} "mkdir -p ~/app/deployment"
                                scp -i vault_ssh_key -o StrictHostKeyChecking=no -r docker/* ${DEPLOY_TARGET}:~/app/deployment/
                                ssh -i vault_ssh_key -o StrictHostKeyChecking=no ${DEPLOY_TARGET} "
                                    cd ~/app/deployment && \\
                                    chmod +x deploy.sh && \\
                                    ./deploy.sh
                                "
                                
                                # Clean up workspace
                                rm -f vault_ssh_key vault_ssh_key-cert.pub ephemeral_keys.json docker/deploy.sh
                            '''
                        }
                    }
                }
            }
        }
    }
    
    post {
        always {
            // Clean up the local images to save disk space on the Jenkins agent
            sh "docker rmi ${env.DOCKER_REGISTRY}/logger_gateway:${env.IMAGE_TAG} || true"
            sh "docker rmi ${env.DOCKER_REGISTRY}/validation_worker:${env.IMAGE_TAG} || true"
        }
    }
}
