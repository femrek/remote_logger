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
                // Fetch the target server address from the KV engine
                withVault(vaultSecrets: [[
                    path: 'secret/data/jenkins/deploy_config',
                    secretValues: [[envVar: 'DEPLOY_TARGET', vaultKey: 'target']]
                ]]) {
                    // Generate the ephemeral SSH key using the SSH engine
                    withVault(vaultSecrets: [[
                        engineVersion: 2, 
                        path: 'ssh/issue/jenkins-deploy', 
                        secretValues: [
                            [envVar: 'SSH_CERT', vaultKey: 'signed_key'],
                            [envVar: 'SSH_KEY',  vaultKey: 'private_key']
                        ]
                    ]]) {
                        script {
                            echo "Deploying to production target via Vault-signed SSH..."
                            
                            writeFile file: 'vault_ssh_key', text: env.SSH_KEY
                            writeFile file: 'vault_ssh_key-cert.pub', text: env.SSH_CERT
                            sh 'chmod 600 vault_ssh_key'
                            
                            // Use the DEPLOY_TARGET variable instead of a hardcoded string
                            sh '''
                                cp docker/deploy.sh.template docker/deploy.sh
                                ssh -i vault_ssh_key -o StrictHostKeyChecking=no ${DEPLOY_TARGET} "mkdir -p ~/app/deployment"
                                scp -i vault_ssh_key -o StrictHostKeyChecking=no -r docker/* ${DEPLOY_TARGET}:~/app/deployment/
                                ssh -i vault_ssh_key -o StrictHostKeyChecking=no ${DEPLOY_TARGET} "
                                    cd ~/app/deployment && \\
                                    chmod +x deploy.sh && \\
                                    ./deploy.sh
                                "
                            '''
                            
                            sh 'rm -f vault_ssh_key vault_ssh_key-cert.pub docker/deploy.sh'
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
