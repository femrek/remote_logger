pipeline {
    agent any

    environment {
        // Automatically mapped to ghcr.io based on docker-compose.yaml configuration
        DOCKER_REGISTRY = 'ghcr.io/femrek' 
        
        // env.TAG_NAME is automatically populated by Jenkins when a tag is built
        IMAGE_TAG = "${env.TAG_NAME}" 
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
                    docker.withRegistry('https://ghcr.io', 'github-credentials') {
                        gatewayImage.push()
                        gatewayImage.push('latest')
                        
                        workerImage.push()
                        workerImage.push('latest') 
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
