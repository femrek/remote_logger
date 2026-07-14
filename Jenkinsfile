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
        stage('CI Pipeline') {
            agent {
                docker {
                    image 'rust:bookworm'
                    // Run as root inside the container to guarantee permissions to access /var/run/docker.sock
                    args '-u root -v /var/run/docker.sock:/var/run/docker.sock --network host'
                }
            }
            environment {
                CARGO_HOME = "${WORKSPACE}/.cargo"
            }
            stages {
                stage('Setup') {
                    steps {
                        script {
                            echo 'Installing cmake for rdkafka-sys build...'
                            sh 'apt-get update && apt-get install -y cmake'
                        }
                    }
                }

                stage('Test logger_gateway') {
                    steps {
                        script {
                            echo 'Running logger_gateway unit tests...'
                            sh 'cd logger_gateway && cargo test'
                        }
                    }
                }

                stage('Test validation_worker') {
                    steps {
                        script {
                            echo 'Running validation_worker unit and integration tests...'
                            sh 'cd validation_worker && cargo test'
                        }
                    }
                }
            }
        }

        stage('Verify Tag') {
            when {
                buildingTag()
            }
            steps {
                script {
                    echo "Building Docker images for tag: ${env.TAG_NAME}"
                }
            }
        }

        stage('Build Docker Images') {
            when {
                buildingTag()
            }
            steps {
                script {
                    // Build logger_gateway from its context and Dockerfile
                    echo "Building logger_gateway image..."
                    gatewayImage = docker.build("${DOCKER_REGISTRY}/logger_gateway:${IMAGE_TAG}", "-f logger_gateway/docker/Dockerfile logger_gateway")
                    
                    // Build validation_worker from its context and Dockerfile
                    echo "Building validation_worker image..."
                    workerImage = docker.build("${DOCKER_REGISTRY}/validation_worker:${IMAGE_TAG}", "-f validation_worker/Dockerfile validation_worker")

                    // Build gateway_admin from its context and Dockerfile
                    echo "Building gateway_admin image..."
                    adminImage = docker.build("${DOCKER_REGISTRY}/gateway_admin:${IMAGE_TAG}", "-f gateway_admin/Dockerfile gateway_admin")
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

                        adminImage.push()
                        adminImage.push('latest')
                    }
                }
            }
        }
    }
    
    post {
        always {
            script {
                // Only clean up images if a tag was built and images were generated
                if (env.TAG_NAME) {
                    sh "docker rmi ${env.DOCKER_REGISTRY}/logger_gateway:${env.IMAGE_TAG} || true"
                    sh "docker rmi ${env.DOCKER_REGISTRY}/validation_worker:${env.IMAGE_TAG} || true"
                    sh "docker rmi ${env.DOCKER_REGISTRY}/gateway_admin:${env.IMAGE_TAG} || true"
                }
            }
        }
    }
}
